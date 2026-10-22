use super::*;
const RAW_LIMIT: usize = 1024 * 1024;
pub(crate) fn decode(raw: &[u8]) -> Result<Vec<String>> {
    ensure!(
        raw.len() <= RAW_LIMIT && !raw.starts_with(&[0xef, 0xbb, 0xbf]) && !raw.contains(&0),
        "native_response_raw_unsupported"
    );
    let text = std::str::from_utf8(raw)?;
    let mut out = Vec::new();
    let mut current = String::new();
    let mut active = false;
    let mut quote = None;
    let mut iter = text.chars();
    while let Some(c) = iter.next() {
        if c == '\\' {
            current.push(iter.next().context("native_response_trailing_escape")?);
            active = true;
        } else if let Some(q) = quote {
            if c == q {
                quote = None
            } else {
                current.push(c)
            }
        } else if c == '\'' || c == '"' {
            quote = Some(c);
            active = true;
        } else if matches!(c, ' ' | '\t' | '\r' | '\n') {
            if active {
                out.push(std::mem::take(&mut current));
                active = false;
            }
        } else {
            current.push(c);
            active = true;
        }
    }
    ensure!(quote.is_none(), "native_response_unterminated_quote");
    if active {
        out.push(current)
    }
    ensure!(out.len() <= 10000, "native_response_argument_limit");
    Ok(out)
}
fn resolve(cwd: &Path, s: &str) -> Result<PathBuf> {
    let p = Path::new(s);
    let p = if p.is_absolute() {
        p.to_owned()
    } else {
        cwd.join(p)
    };
    no_symlinks(&p)?;
    Ok(p)
}
pub(crate) fn expand<F>(
    args: &[String],
    cwd: &Path,
    mut load: F,
) -> Result<(Vec<String>, Vec<CodeNativeResponseExpansionFact>)>
where
    F: FnMut(&Path) -> Result<(CodeNativeFileBinding, ArtifactReference, Vec<u8>)>,
{
    struct Walker<'a, F> {
        load: &'a mut F,
        cwd: &'a Path,
        out: Vec<String>,
        facts: Vec<CodeNativeResponseExpansionFact>,
        stack: BTreeSet<PathBuf>,
        total: usize,
    }
    impl<F: FnMut(&Path) -> Result<(CodeNativeFileBinding, ArtifactReference, Vec<u8>)>> Walker<'_, F> {
        fn walk(&mut self, args: &[String], depth: u64, parent: Option<u64>) -> Result<()> {
            ensure!(depth <= 8, "native_response_depth_limit");
            for (i, arg) in args.iter().enumerate() {
                ensure!(!arg.contains('\0'), "native_response_argument_nul");
                if let Some(name) = arg.strip_prefix('@') {
                    ensure!(!name.is_empty(), "native_response_path_empty");
                    let path = resolve(self.cwd, name)?;
                    ensure!(self.stack.insert(path.clone()), "native_response_cycle");
                    let (file, raw_bytes, raw) = (self.load)(&path)?;
                    self.total = self
                        .total
                        .checked_add(raw.len())
                        .context("native_response_size_overflow")?;
                    ensure!(self.total <= RAW_LIMIT, "native_response_total_limit");
                    ensure!(
                        file.path == path_text(&path)?
                            && file.sha256 == sha256(&raw)
                            && file.bytes == raw.len() as u64,
                        "native_response_file_mismatch"
                    );
                    let decoded = decode(&raw)?;
                    let occurrence = self.facts.len() as u64;
                    let index = self.facts.len();
                    let start = self.out.len() as u64;
                    self.facts.push(CodeNativeResponseExpansionFact {
                        occurrence,
                        parent_occurrence: parent,
                        received_argument_ordinal: i as u64,
                        depth,
                        reference: arg.clone(),
                        caller_cwd: path_text(self.cwd)?,
                        file,
                        raw_bytes,
                        decoded_arguments: decoded.clone(),
                        expansion_start: start,
                        expansion_end: start,
                    });
                    self.walk(&decoded, depth + 1, Some(occurrence))?;
                    self.facts[index].expansion_end = self.out.len() as u64;
                    self.stack.remove(&path);
                } else {
                    self.out.push(arg.clone());
                    ensure!(self.out.len() <= 10000, "native_response_argument_limit");
                }
            }
            Ok(())
        }
    }
    let mut walker = Walker {
        load: &mut load,
        cwd,
        out: Vec::new(),
        facts: Vec::new(),
        stack: BTreeSet::new(),
        total: 0,
    };
    walker.walk(args, 0, None)?;
    Ok((walker.out, walker.facts))
}
