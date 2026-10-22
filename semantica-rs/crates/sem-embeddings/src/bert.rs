//! BERT sentence embeddings with candle, loaded from a local model directory.
//!
//! This is what `sentence_transformers.SentenceTransformer` and `fastembed.TextEmbedding` do
//! for BERT-architecture models: tokenize (WordPiece, `[CLS] ... [SEP]`, truncation to the
//! model's maximum sequence length, padding to the longest text of the batch), run the encoder,
//! pool the token states with the attention mask (CLS token, mean, max or mean / sqrt(length),
//! concatenated in sentence-transformers' order) and optionally L2-normalise.
//!
//! A directory in sentence-transformers layout (`modules.json`, `1_Pooling/config.json`,
//! `2_Normalize`, `sentence_bert_config.json`) is read the way sentence-transformers reads it;
//! a plain Hugging Face model directory gets sentence-transformers' default (mean pooling).
//! Weights come from `model.safetensors` or `pytorch_model.bin`; the tokenizer from
//! `tokenizer.json`, or is built from `vocab.txt`. Nothing is ever downloaded.

use std::path::{Path, PathBuf};

use candle_core::{D, DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::strip;
use serde_json::Value;
use tokenizers::models::wordpiece::WordPiece;
use tokenizers::normalizers::bert::BertNormalizer;
use tokenizers::pre_tokenizers::bert::BertPreTokenizer;
use tokenizers::processors::bert::BertProcessing;
use tokenizers::{
    PaddingDirection, PaddingParams, PaddingStrategy, Tokenizer, TruncationDirection,
    TruncationParams, TruncationStrategy,
};

/// Which token-pooling modes are active (sentence-transformers' `Pooling` module); the
/// outputs are concatenated in the order CLS, max, mean, mean / sqrt(length).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PoolingModes {
    /// `pooling_mode_cls_token`.
    pub cls: bool,
    /// `pooling_mode_max_tokens`.
    pub max: bool,
    /// `pooling_mode_mean_tokens`.
    pub mean: bool,
    /// `pooling_mode_mean_sqrt_len_tokens`.
    pub mean_sqrt_len: bool,
}

impl PoolingModes {
    /// CLS-token pooling only.
    pub const CLS: PoolingModes = PoolingModes {
        cls: true,
        max: false,
        mean: false,
        mean_sqrt_len: false,
    };
    /// Mean pooling only (sentence-transformers' default).
    pub const MEAN: PoolingModes = PoolingModes {
        cls: false,
        max: false,
        mean: true,
        mean_sqrt_len: false,
    };

    fn count(&self) -> usize {
        [self.cls, self.max, self.mean, self.mean_sqrt_len]
            .iter()
            .filter(|on| **on)
            .count()
    }
}

/// How the model directory is interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    /// sentence-transformers: `modules.json` decides pooling and normalisation.
    SentenceTransformers,
    /// fastembed: pooling and normalisation are fixed by the model's description.
    Fixed {
        pooling: PoolingModes,
        normalize: bool,
    },
}

/// A BERT encoder plus tokenizer and pooling, ready to embed sentences.
pub(crate) struct BertSentenceEncoder {
    model: BertModel,
    tokenizer: Tokenizer,
    pooling: PoolingModes,
    normalize: bool,
    do_lower_case: bool,
    hidden_size: usize,
    max_seq_length: usize,
    device: Device,
    dir: PathBuf,
}

fn runtime(err: impl std::fmt::Display) -> SemanticaError {
    SemanticaError::Runtime(err.to_string())
}

fn os_error(message: String) -> SemanticaError {
    SemanticaError::Os(sem_utils::exceptions::OsError::new("OSError", message))
}

fn read_json(path: &Path) -> Result<Value, SemanticaError> {
    let text = std::fs::read_to_string(path).map_err(|e| SemanticaError::from_io(&e, Some(path)))?;
    serde_json::from_str(&text).map_err(|e| SemanticaError::JsonDecode(e.to_string()))
}

/// `device` as sentence-transformers accepts it; the port runs on the CPU only.
fn parse_device(device: &str) -> Result<Device, SemanticaError> {
    match device.to_ascii_lowercase().as_str() {
        "cpu" => Ok(Device::Cpu),
        other => Err(runtime(format!(
            "device '{other}' is not available: the Rust port runs models on the CPU"
        ))),
    }
}

/// The `type` of a `modules.json` entry, by its last dotted component.
fn module_kind(entry: &Value) -> String {
    entry
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_string()
}

struct StLayout {
    transformer_dir: PathBuf,
    pooling: PoolingModes,
    normalize: bool,
}

fn read_st_layout(dir: &Path) -> Result<StLayout, SemanticaError> {
    let modules_path = dir.join("modules.json");
    if !modules_path.is_file() {
        // A plain Hugging Face model: Transformer + mean Pooling.
        return Ok(StLayout {
            transformer_dir: dir.to_path_buf(),
            pooling: PoolingModes::MEAN,
            normalize: false,
        });
    }
    let modules = read_json(&modules_path)?;
    let entries = modules.as_array().cloned().unwrap_or_default();
    let mut transformer_dir = None;
    let mut pooling = None;
    let mut normalize = false;
    for entry in &entries {
        let path = entry.get("path").and_then(Value::as_str).unwrap_or("");
        let module_dir = if path.is_empty() {
            dir.to_path_buf()
        } else {
            dir.join(path)
        };
        match module_kind(entry).as_str() {
            "Transformer" => transformer_dir = Some(module_dir),
            "Pooling" => pooling = Some(read_pooling_config(&module_dir.join("config.json"))?),
            "Normalize" => normalize = true,
            other => {
                return Err(runtime(format!(
                    "sentence-transformers module '{other}' is not supported by the Rust port"
                )));
            }
        }
    }
    Ok(StLayout {
        transformer_dir: transformer_dir
            .ok_or_else(|| runtime("modules.json has no Transformer module"))?,
        pooling: pooling.unwrap_or(PoolingModes::MEAN),
        normalize,
    })
}

fn read_pooling_config(path: &Path) -> Result<PoolingModes, SemanticaError> {
    let config = read_json(path)?;
    let flag = |key: &str| config.get(key).and_then(Value::as_bool).unwrap_or(false);
    if flag("pooling_mode_weightedmean_tokens") || flag("pooling_mode_lasttoken") {
        return Err(runtime(
            "weighted-mean and last-token pooling are not supported by the Rust port",
        ));
    }
    let mut modes = PoolingModes {
        cls: flag("pooling_mode_cls_token"),
        max: flag("pooling_mode_max_tokens"),
        mean: flag("pooling_mode_mean_tokens"),
        mean_sqrt_len: flag("pooling_mode_mean_sqrt_len_tokens"),
    };
    if let Some(mode) = config.get("pooling_mode").and_then(Value::as_str) {
        modes = match mode {
            "cls" => PoolingModes::CLS,
            "mean" => PoolingModes::MEAN,
            "max" => PoolingModes {
                max: true,
                ..PoolingModes::default()
            },
            "mean_sqrt_len_tokens" => PoolingModes {
                mean_sqrt_len: true,
                ..PoolingModes::default()
            },
            other => {
                return Err(runtime(format!(
                    "pooling mode '{other}' is not supported by the Rust port"
                )));
            }
        };
    }
    if modes.count() == 0 {
        return Err(runtime("the Pooling module has no pooling mode enabled"));
    }
    Ok(modes)
}

fn build_wordpiece_tokenizer(dir: &Path, lowercase: bool) -> Result<Tokenizer, SemanticaError> {
    let vocab = dir.join("vocab.txt");
    let vocab_str = vocab.to_string_lossy().into_owned();
    let model = WordPiece::from_file(&vocab_str)
        .unk_token("[UNK]".to_string())
        .build()
        .map_err(runtime)?;
    let mut tokenizer = Tokenizer::new(model);
    let sep = tokenizer
        .token_to_id("[SEP]")
        .ok_or_else(|| runtime("vocab.txt has no [SEP] token"))?;
    let cls = tokenizer
        .token_to_id("[CLS]")
        .ok_or_else(|| runtime("vocab.txt has no [CLS] token"))?;
    tokenizer.with_normalizer(Some(BertNormalizer::new(true, true, None, lowercase)));
    tokenizer.with_pre_tokenizer(Some(BertPreTokenizer));
    tokenizer.with_post_processor(Some(BertProcessing::new(
        ("[SEP]".to_string(), sep),
        ("[CLS]".to_string(), cls),
    )));
    Ok(tokenizer)
}

fn load_tokenizer(dir: &Path) -> Result<Tokenizer, SemanticaError> {
    let path = dir.join("tokenizer.json");
    if path.is_file()
        && let Ok(tokenizer) = Tokenizer::from_file(&path)
    {
        return Ok(tokenizer);
    }
    if dir.join("vocab.txt").is_file() {
        let lowercase = read_json(&dir.join("tokenizer_config.json"))
            .ok()
            .and_then(|c| c.get("do_lower_case").and_then(Value::as_bool))
            .unwrap_or(true);
        return build_wordpiece_tokenizer(dir, lowercase);
    }
    Err(os_error(format!(
        "Can't load tokenizer for '{}': no tokenizer.json or vocab.txt",
        dir.display()
    )))
}

fn load_weights(dir: &Path, device: &Device) -> Result<VarBuilder<'static>, SemanticaError> {
    let safetensors = dir.join("model.safetensors");
    if safetensors.is_file() {
        let tensors = candle_core::safetensors::load(&safetensors, device).map_err(runtime)?;
        return Ok(VarBuilder::from_tensors(tensors, DType::F32, device));
    }
    let pth = dir.join("pytorch_model.bin");
    if pth.is_file() {
        return VarBuilder::from_pth(&pth, DType::F32, device).map_err(runtime);
    }
    Err(os_error(format!(
        "{} does not appear to have a file named pytorch_model.bin or model.safetensors.",
        dir.display()
    )))
}

impl BertSentenceEncoder {
    /// Load the model in `dir` for `device` (only `"cpu"`).
    pub(crate) fn load(
        dir: &Path,
        device: &str,
        layout: Layout,
    ) -> Result<BertSentenceEncoder, SemanticaError> {
        let device = parse_device(device)?;
        let (transformer_dir, pooling, normalize) = match layout {
            Layout::SentenceTransformers => {
                let st = read_st_layout(dir)?;
                (st.transformer_dir, st.pooling, st.normalize)
            }
            Layout::Fixed { pooling, normalize } => (dir.to_path_buf(), pooling, normalize),
        };
        let config_value = read_json(&transformer_dir.join("config.json"))?;
        if let Some(model_type) = config_value.get("model_type").and_then(Value::as_str)
            && model_type != "bert"
        {
            return Err(runtime(format!(
                "model type '{model_type}' is not supported by the Rust port (BERT models only)"
            )));
        }
        let config: BertConfig = serde_json::from_value(config_value).map_err(runtime)?;

        // sentence_bert_config.json: max_seq_length / do_lower_case
        let st_config = read_json(&transformer_dir.join("sentence_bert_config.json"))
            .or_else(|_| read_json(&dir.join("sentence_bert_config.json")))
            .ok();
        let tokenizer_config = read_json(&transformer_dir.join("tokenizer_config.json")).ok();
        let mut max_seq_length = st_config
            .as_ref()
            .and_then(|c| c.get("max_seq_length").and_then(Value::as_u64))
            .map(|v| v as usize)
            .or_else(|| {
                tokenizer_config
                    .as_ref()
                    .and_then(|c| c.get("model_max_length").and_then(Value::as_f64))
                    .filter(|v| *v > 0.0 && *v < 1e6)
                    .map(|v| v as usize)
            })
            .unwrap_or(config.max_position_embeddings);
        max_seq_length = max_seq_length.min(config.max_position_embeddings);
        let do_lower_case = st_config
            .as_ref()
            .and_then(|c| c.get("do_lower_case").and_then(Value::as_bool))
            .unwrap_or(false);

        let mut tokenizer = load_tokenizer(&transformer_dir)?;
        let pad_id = tokenizer
            .token_to_id("[PAD]")
            .unwrap_or(config.pad_token_id as u32);
        tokenizer
            .with_truncation(Some(TruncationParams {
                direction: TruncationDirection::Right,
                max_length: max_seq_length,
                strategy: TruncationStrategy::LongestFirst,
                stride: 0,
            }))
            .map_err(runtime)?;
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            direction: PaddingDirection::Right,
            pad_to_multiple_of: None,
            pad_id,
            pad_type_id: 0,
            pad_token: "[PAD]".to_string(),
        }));

        let vb = load_weights(&transformer_dir, &device)?;
        let model = BertModel::load(vb, &config).map_err(runtime)?;
        Ok(BertSentenceEncoder {
            model,
            tokenizer,
            pooling,
            normalize,
            do_lower_case,
            hidden_size: config.hidden_size,
            max_seq_length,
            device,
            dir: dir.to_path_buf(),
        })
    }

    /// The embedding dimension (`get_sentence_embedding_dimension()`).
    pub(crate) fn dimension(&self) -> usize {
        self.hidden_size * self.pooling.count()
    }

    /// The directory the model was loaded from.
    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    /// The maximum number of tokens per text.
    pub(crate) fn max_seq_length(&self) -> usize {
        self.max_seq_length
    }

    /// Embed `texts` in batches of `batch_size`; `normalize_embeddings` adds an L2
    /// normalisation after the model's own modules (sentence-transformers' `encode` flag).
    pub(crate) fn encode(
        &self,
        texts: &[String],
        batch_size: usize,
        normalize_embeddings: bool,
    ) -> Result<Vec<Vec<f32>>, SemanticaError> {
        let mut out = Vec::with_capacity(texts.len());
        for batch in texts.chunks(batch_size.max(1)) {
            out.extend(self.encode_batch(batch, normalize_embeddings)?);
        }
        Ok(out)
    }

    fn encode_batch(
        &self,
        texts: &[String],
        normalize_embeddings: bool,
    ) -> Result<Vec<Vec<f32>>, SemanticaError> {
        // sentence-transformers strips (and optionally lower-cases) every text first.
        let prepared: Vec<String> = texts
            .iter()
            .map(|t| {
                let s = strip(t);
                if self.do_lower_case {
                    s.to_lowercase()
                } else {
                    s.to_string()
                }
            })
            .collect();
        let inputs: Vec<&str> = prepared.iter().map(String::as_str).collect();
        let encodings = self.tokenizer.encode_batch(inputs, true).map_err(runtime)?;
        let batch = encodings.len();
        let seq_len = encodings.first().map_or(0, |e| e.get_ids().len());
        let mut ids = Vec::with_capacity(batch * seq_len);
        let mut type_ids = Vec::with_capacity(batch * seq_len);
        let mut mask = Vec::with_capacity(batch * seq_len);
        for encoding in &encodings {
            ids.extend_from_slice(encoding.get_ids());
            type_ids.extend_from_slice(encoding.get_type_ids());
            mask.extend_from_slice(encoding.get_attention_mask());
        }
        let shape = (batch, seq_len);
        let ids = Tensor::from_vec(ids, shape, &self.device).map_err(runtime)?;
        let type_ids = Tensor::from_vec(type_ids, shape, &self.device).map_err(runtime)?;
        let mask = Tensor::from_vec(mask, shape, &self.device).map_err(runtime)?;
        let hidden = self
            .model
            .forward(&ids, &type_ids, Some(&mask))
            .map_err(runtime)?;
        let pooled = self.pool(&hidden, &mask).map_err(runtime)?;
        let pooled = if self.normalize {
            l2_normalize(&pooled).map_err(runtime)?
        } else {
            pooled
        };
        let pooled = if normalize_embeddings {
            l2_normalize(&pooled).map_err(runtime)?
        } else {
            pooled
        };
        pooled.to_vec2::<f32>().map_err(runtime)
    }

    fn pool(&self, hidden: &Tensor, mask: &Tensor) -> candle_core::Result<Tensor> {
        let mask = mask.to_dtype(DType::F32)?.unsqueeze(2)?; // (b, l, 1)
        let mut parts = Vec::new();
        if self.pooling.cls {
            parts.push(hidden.i((.., 0))?.contiguous()?);
        }
        if self.pooling.max {
            let penalty = ((mask.ones_like()? - &mask)? * -1e9)?;
            parts.push(hidden.broadcast_add(&penalty)?.max(1)?);
        }
        if self.pooling.mean || self.pooling.mean_sqrt_len {
            let summed = hidden.broadcast_mul(&mask)?.sum(1)?; // (b, h)
            let counts = mask.sum(1)?.clamp(1e-9f32, f32::MAX)?; // (b, 1)
            if self.pooling.mean {
                parts.push(summed.broadcast_div(&counts)?);
            }
            if self.pooling.mean_sqrt_len {
                parts.push(summed.broadcast_div(&counts.sqrt()?)?);
            }
        }
        Tensor::cat(&parts, D::Minus1)
    }
}

/// `torch.nn.functional.normalize(x, p=2, dim=1)`: `x / max(||x||, 1e-12)`.
fn l2_normalize(x: &Tensor) -> candle_core::Result<Tensor> {
    let norms = x.sqr()?.sum_keepdim(D::Minus1)?.sqrt()?.clamp(1e-12f32, f32::MAX)?;
    x.broadcast_div(&norms)
}
