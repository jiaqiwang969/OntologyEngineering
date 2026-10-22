//! The `numpy.ndarray` values the embeddings module passes around.
//!
//! The original hands embeddings around as numpy arrays: 1-D vectors and 2-D batches of
//! `float32` (what every model returns) or `float64` (what `np.array([...])` builds from Python
//! floats). [`NdArray`] keeps the shape and the dtype so that the arithmetic the module does —
//! means, maxima, norms, dot products, softmax weights — runs in the same precision as numpy.
//!
//! Reductions follow numpy's evaluation order, which is deterministic and therefore reproduced
//! exactly: a 1-D sum (or a reduction along a contiguous axis) is `0.0 + pairwise_sum(...)`
//! (numpy's 8-accumulator pairwise summation), a sum over axis 0 of a 2-D array with more than
//! one column adds the rows one after another, starting from `0.0`. Dot products and vector
//! norms go through BLAS in numpy (Apple Accelerate on the reference machine), whose internal
//! order is not documented; here a `float32` dot product is accumulated in `float64` and
//! rounded once, a `float64` one is summed in order, so those results can differ from the
//! original in the last bit.

use std::fmt;

use sem_utils::SemanticaError;
use serde::{Serialize, Serializer};
use serde_json::Value;

/// A numpy floating-point dtype.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DType {
    /// `numpy.float32`.
    Float32,
    /// `numpy.float64`.
    Float64,
}

impl DType {
    /// `str(array.dtype)`: `"float32"` or `"float64"`.
    pub fn name(self) -> &'static str {
        match self {
            DType::Float32 => "float32",
            DType::Float64 => "float64",
        }
    }

    /// numpy's result type for an operation on two arrays of these dtypes.
    pub fn promote(self, other: DType) -> DType {
        if self == DType::Float64 || other == DType::Float64 {
            DType::Float64
        } else {
            DType::Float32
        }
    }
}

impl fmt::Display for DType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, PartialEq)]
enum Data {
    F32(Vec<f32>),
    F64(Vec<f64>),
}

/// A 0-, 1- or 2-dimensional `float32`/`float64` array stored in row-major order (the
/// `numpy.ndarray` of the original).
///
/// A 0-dimensional array stands for the numpy scalar a reduction of a 1-D array returns.
#[derive(Clone, PartialEq)]
pub struct NdArray {
    shape: Vec<usize>,
    data: Data,
}

/// The floating-point element types an [`NdArray`] can hold, with the arithmetic numpy uses
/// for them.
pub(crate) trait Float:
    Copy
    + PartialEq
    + PartialOrd
    + fmt::Debug
    + Send
    + Sync
    + 'static
    + std::ops::Add<Output = Self>
    + std::ops::Sub<Output = Self>
    + std::ops::Mul<Output = Self>
    + std::ops::Div<Output = Self>
{
    const ZERO: Self;
    const ONE: Self;
    const NAN: Self;
    const DTYPE: DType;

    fn from_f64(x: f64) -> Self;
    fn to_f64(self) -> f64;
    fn sqrt(self) -> Self;
    fn exp(self) -> Self;
    fn is_nan(self) -> bool;
    /// BLAS `?dot` (what `np.dot` / `np.linalg.norm` call for contiguous vectors).
    fn dot(a: &[Self], b: &[Self]) -> Self;
    /// `sum / count` as `np.mean` computes it (`true_divide` by an `intp`, i.e. in `float64`,
    /// then cast back to the array dtype).
    fn div_count(self, count: usize) -> Self;
    fn view(array: &NdArray) -> Option<&[Self]>;
    fn wrap(shape: Vec<usize>, data: Vec<Self>) -> NdArray;
}

impl Float for f32 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
    const NAN: Self = f32::NAN;
    const DTYPE: DType = DType::Float32;

    fn from_f64(x: f64) -> Self {
        x as f32
    }
    fn to_f64(self) -> f64 {
        self as f64
    }
    fn sqrt(self) -> Self {
        f32::sqrt(self)
    }
    fn exp(self) -> Self {
        f32::exp(self)
    }
    fn is_nan(self) -> bool {
        f32::is_nan(self)
    }
    fn dot(a: &[Self], b: &[Self]) -> Self {
        // Products of two floats are exact in f64; one final rounding.
        let mut sum = 0.0f64;
        for (x, y) in a.iter().zip(b) {
            sum += (*x as f64) * (*y as f64);
        }
        sum as f32
    }
    fn div_count(self, count: usize) -> Self {
        (self as f64 / count as f64) as f32
    }
    fn view(array: &NdArray) -> Option<&[Self]> {
        match &array.data {
            Data::F32(v) => Some(v),
            Data::F64(_) => None,
        }
    }
    fn wrap(shape: Vec<usize>, data: Vec<Self>) -> NdArray {
        NdArray {
            shape,
            data: Data::F32(data),
        }
    }
}

impl Float for f64 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
    const NAN: Self = f64::NAN;
    const DTYPE: DType = DType::Float64;

    fn from_f64(x: f64) -> Self {
        x
    }
    fn to_f64(self) -> f64 {
        self
    }
    fn sqrt(self) -> Self {
        f64::sqrt(self)
    }
    fn exp(self) -> Self {
        f64::exp(self)
    }
    fn is_nan(self) -> bool {
        f64::is_nan(self)
    }
    fn dot(a: &[Self], b: &[Self]) -> Self {
        let mut sum = 0.0f64;
        for (x, y) in a.iter().zip(b) {
            sum += x * y;
        }
        sum
    }
    fn div_count(self, count: usize) -> Self {
        self / count as f64
    }
    fn view(array: &NdArray) -> Option<&[Self]> {
        match &array.data {
            Data::F64(v) => Some(v),
            Data::F32(_) => None,
        }
    }
    fn wrap(shape: Vec<usize>, data: Vec<Self>) -> NdArray {
        NdArray {
            shape,
            data: Data::F64(data),
        }
    }
}

/// Runs `$body` with `$v` bound to the array's elements as `&[$t]`, `$t` being the element
/// type of the array's dtype.
macro_rules! with_float {
    ($array:expr, |$v:ident : $t:ident| $body:expr) => {{
        let array: &$crate::ndarray::NdArray = $array;
        match array.dtype() {
            $crate::ndarray::DType::Float32 => {
                #[allow(dead_code)]
                type $t = f32;
                let $v: &[f32] = <f32 as $crate::ndarray::Float>::view(array)
                    .expect("dtype matches storage");
                $body
            }
            $crate::ndarray::DType::Float64 => {
                #[allow(dead_code)]
                type $t = f64;
                let $v: &[f64] = <f64 as $crate::ndarray::Float>::view(array)
                    .expect("dtype matches storage");
                $body
            }
        }
    }};
}
pub(crate) use with_float;

/// numpy's `pairwise_sum` (8 accumulators, blocks of 128, recursive halving), without the
/// reduction's initial value.
pub(crate) fn pairwise_sum<T: Float>(a: &[T]) -> T {
    let n = a.len();
    if n < 8 {
        let mut res = T::ZERO;
        for &x in a {
            res = res + x;
        }
        res
    } else if n <= 128 {
        let mut r = [T::ZERO; 8];
        r.copy_from_slice(&a[..8]);
        let mut i = 8;
        while i < n - (n % 8) {
            for j in 0..8 {
                r[j] = r[j] + a[i + j];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        while i < n {
            res = res + a[i];
            i += 1;
        }
        res
    } else {
        let mut n2 = n / 2;
        n2 -= n2 % 8;
        pairwise_sum(&a[..n2]) + pairwise_sum(&a[n2..])
    }
}

/// `np.add.reduce` of a contiguous 1-D run: the identity `0.0` plus the pairwise sum.
pub(crate) fn add_reduce<T: Float>(a: &[T]) -> T {
    T::ZERO + pairwise_sum(a)
}

/// `np.sum(m, axis=0)` of a C-contiguous `(rows, cols)` array.
///
/// numpy adds whole rows one after another (starting from `0.0`); a single column is a
/// contiguous run and is reduced pairwise instead.
pub(crate) fn sum_axis0<T: Float>(data: &[T], rows: usize, cols: usize) -> Vec<T> {
    if cols == 1 {
        return vec![add_reduce(&data[..rows])];
    }
    let mut out = vec![T::ZERO; cols];
    for r in 0..rows {
        let row = &data[r * cols..(r + 1) * cols];
        for (o, &x) in out.iter_mut().zip(row) {
            *o = *o + x;
        }
    }
    out
}

/// `np.maximum` of two elements (NaN propagates).
pub(crate) fn maximum<T: Float>(a: T, b: T) -> T {
    if a.is_nan() {
        a
    } else if b.is_nan() {
        b
    } else if b > a {
        b
    } else {
        a
    }
}

/// `np.linalg.norm(v)` of a contiguous vector: `sqrt(v.dot(v))`.
pub(crate) fn vector_norm<T: Float>(v: &[T]) -> T {
    T::dot(v, v).sqrt()
}

/// `np.linalg.norm(m, axis=1)`: `sqrt(add.reduce(m * m, axis=1))`, one value per row.
pub(crate) fn row_norms<T: Float>(data: &[T], rows: usize, cols: usize) -> Vec<T> {
    (0..rows)
        .map(|r| {
            let squares: Vec<T> = data[r * cols..(r + 1) * cols]
                .iter()
                .map(|&x| x * x)
                .collect();
            add_reduce(&squares).sqrt()
        })
        .collect()
}

/// numpy's `repr` of a shape tuple: `()`, `(3,)`, `(2, 3)`.
pub(crate) fn shape_repr(shape: &[usize]) -> String {
    match shape {
        [] => "()".to_string(),
        [n] => format!("({n},)"),
        dims => format!(
            "({})",
            dims.iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn inhomogeneous(n: usize) -> SemanticaError {
    SemanticaError::Value(format!(
        "setting an array element with a sequence. The requested array has an inhomogeneous \
         shape after 1 dimensions. The detected shape was ({n},) + inhomogeneous part."
    ))
}

impl NdArray {
    /// A 1-D `float32` array (`np.array(values, dtype=np.float32)`).
    pub fn from_vec_f32(values: Vec<f32>) -> NdArray {
        NdArray {
            shape: vec![values.len()],
            data: Data::F32(values),
        }
    }

    /// A 1-D `float64` array (`np.array(values)` of Python floats).
    pub fn from_vec_f64(values: Vec<f64>) -> NdArray {
        NdArray {
            shape: vec![values.len()],
            data: Data::F64(values),
        }
    }

    /// A 0-D `float32` value (a `numpy.float32` scalar).
    pub fn scalar_f32(value: f32) -> NdArray {
        NdArray {
            shape: Vec::new(),
            data: Data::F32(vec![value]),
        }
    }

    /// A 0-D `float64` value (a `numpy.float64` scalar).
    pub fn scalar_f64(value: f64) -> NdArray {
        NdArray {
            shape: Vec::new(),
            data: Data::F64(vec![value]),
        }
    }

    /// `np.array([])`: an empty 1-D `float64` array.
    pub fn empty() -> NdArray {
        NdArray::from_vec_f64(Vec::new())
    }

    /// An array of the given shape over row-major `float32` data.
    pub fn from_shape_f32(shape: Vec<usize>, data: Vec<f32>) -> Result<NdArray, SemanticaError> {
        Self::check_size(&shape, data.len())?;
        Ok(NdArray {
            shape,
            data: Data::F32(data),
        })
    }

    /// An array of the given shape over row-major `float64` data.
    pub fn from_shape_f64(shape: Vec<usize>, data: Vec<f64>) -> Result<NdArray, SemanticaError> {
        Self::check_size(&shape, data.len())?;
        Ok(NdArray {
            shape,
            data: Data::F64(data),
        })
    }

    fn check_size(shape: &[usize], len: usize) -> Result<(), SemanticaError> {
        if shape.len() > 2 {
            return Err(SemanticaError::Value(format!(
                "only arrays of up to 2 dimensions are supported, got shape {}",
                shape_repr(shape)
            )));
        }
        let size: usize = shape.iter().product();
        if size != len {
            return Err(SemanticaError::Value(format!(
                "cannot reshape array of size {len} into shape {}",
                shape_repr(shape)
            )));
        }
        Ok(())
    }

    /// `np.array(rows, dtype=np.float32)`: a 2-D array (an empty list gives shape `(0,)`).
    /// Rows of different lengths are numpy's "inhomogeneous shape" `ValueError`.
    pub fn from_rows_f32(rows: Vec<Vec<f32>>) -> Result<NdArray, SemanticaError> {
        if rows.is_empty() {
            return Ok(NdArray::from_vec_f32(Vec::new()));
        }
        let cols = rows[0].len();
        if rows.iter().any(|r| r.len() != cols) {
            return Err(inhomogeneous(rows.len()));
        }
        let n = rows.len();
        Ok(NdArray {
            shape: vec![n, cols],
            data: Data::F32(rows.into_iter().flatten().collect()),
        })
    }

    /// `np.array(rows)` of Python floats: a 2-D `float64` array (an empty list gives `(0,)`).
    pub fn from_rows_f64(rows: Vec<Vec<f64>>) -> Result<NdArray, SemanticaError> {
        if rows.is_empty() {
            return Ok(NdArray::empty());
        }
        let cols = rows[0].len();
        if rows.iter().any(|r| r.len() != cols) {
            return Err(inhomogeneous(rows.len()));
        }
        let n = rows.len();
        Ok(NdArray {
            shape: vec![n, cols],
            data: Data::F64(rows.into_iter().flatten().collect()),
        })
    }

    /// `np.array([a, b, ...])` of equally shaped arrays: a new leading axis (scalars become a
    /// 1-D array, vectors a 2-D one). The dtype is the common dtype; an empty list is
    /// `np.array([])`.
    pub fn stack(items: &[NdArray]) -> Result<NdArray, SemanticaError> {
        let Some(first) = items.first() else {
            return Ok(NdArray::empty());
        };
        if items.iter().any(|a| a.shape != first.shape) {
            return Err(inhomogeneous(items.len()));
        }
        if first.ndim() > 1 {
            return Err(SemanticaError::Value(
                "only arrays of up to 2 dimensions are supported".to_string(),
            ));
        }
        let dtype = items
            .iter()
            .fold(first.dtype(), |d, a| d.promote(a.dtype()));
        let mut shape = vec![items.len()];
        shape.extend_from_slice(&first.shape);
        Ok(match dtype {
            DType::Float32 => NdArray {
                shape,
                data: Data::F32(items.iter().flat_map(|a| a.to_vec_f32()).collect()),
            },
            DType::Float64 => NdArray {
                shape,
                data: Data::F64(items.iter().flat_map(|a| a.to_vec_f64()).collect()),
            },
        })
    }

    /// `np.vstack(arrays)`: 1-D arrays become rows; all must have the same number of columns.
    pub fn vstack(arrays: &[NdArray]) -> Result<NdArray, SemanticaError> {
        if arrays.is_empty() {
            return Err(SemanticaError::Value(
                "need at least one array to concatenate".to_string(),
            ));
        }
        let as_2d: Vec<NdArray> = arrays
            .iter()
            .map(|a| match a.ndim() {
                0 => NdArray {
                    shape: vec![1, 1],
                    data: a.data.clone(),
                },
                1 => NdArray {
                    shape: vec![1, a.shape[0]],
                    data: a.data.clone(),
                },
                _ => a.clone(),
            })
            .collect();
        let cols = as_2d[0].shape[1];
        for (i, a) in as_2d.iter().enumerate().skip(1) {
            if a.shape[1] != cols {
                return Err(SemanticaError::Value(format!(
                    "all the input array dimensions except for the concatenation axis must match \
                     exactly, but along dimension 1, the array at index 0 has size {cols} and the \
                     array at index {i} has size {}",
                    a.shape[1]
                )));
            }
        }
        let rows: usize = as_2d.iter().map(|a| a.shape[0]).sum();
        let dtype = as_2d
            .iter()
            .fold(as_2d[0].dtype(), |d, a| d.promote(a.dtype()));
        Ok(match dtype {
            DType::Float32 => NdArray {
                shape: vec![rows, cols],
                data: Data::F32(as_2d.iter().flat_map(|a| a.to_vec_f32()).collect()),
            },
            DType::Float64 => NdArray {
                shape: vec![rows, cols],
                data: Data::F64(as_2d.iter().flat_map(|a| a.to_vec_f64()).collect()),
            },
        })
    }

    /// `array.shape`.
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// `repr(array.shape)`, e.g. `(2, 384)`.
    pub fn shape_repr(&self) -> String {
        shape_repr(&self.shape)
    }

    /// `array.ndim`.
    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    /// `array.size` (number of elements).
    pub fn size(&self) -> usize {
        match &self.data {
            Data::F32(v) => v.len(),
            Data::F64(v) => v.len(),
        }
    }

    /// `len(array)`: the length of the first axis (1 for a 0-D value, which numpy rejects).
    pub fn len(&self) -> usize {
        self.shape.first().copied().unwrap_or(1)
    }

    /// Whether the array has no elements.
    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }

    /// `array.dtype`.
    pub fn dtype(&self) -> DType {
        match self.data {
            Data::F32(_) => DType::Float32,
            Data::F64(_) => DType::Float64,
        }
    }

    /// `array.astype(dtype)` (a `float64` -> `float32` cast rounds to nearest).
    pub fn astype(&self, dtype: DType) -> NdArray {
        match (&self.data, dtype) {
            (Data::F32(_), DType::Float32) | (Data::F64(_), DType::Float64) => self.clone(),
            (Data::F64(v), DType::Float32) => NdArray {
                shape: self.shape.clone(),
                data: Data::F32(v.iter().map(|x| *x as f32).collect()),
            },
            (Data::F32(v), DType::Float64) => NdArray {
                shape: self.shape.clone(),
                data: Data::F64(v.iter().map(|x| *x as f64).collect()),
            },
        }
    }

    fn row_len(&self) -> usize {
        if self.shape.len() <= 1 {
            1
        } else {
            self.shape[1..].iter().product()
        }
    }

    /// `array[index]` along the first axis, `None` when out of range (or for a 0-D value).
    pub fn get(&self, index: usize) -> Option<NdArray> {
        if self.ndim() == 0 || index >= self.shape[0] {
            return None;
        }
        let w = self.row_len();
        let shape = self.shape[1..].to_vec();
        Some(match &self.data {
            Data::F32(v) => NdArray {
                shape,
                data: Data::F32(v[index * w..(index + 1) * w].to_vec()),
            },
            Data::F64(v) => NdArray {
                shape,
                data: Data::F64(v[index * w..(index + 1) * w].to_vec()),
            },
        })
    }

    /// `array[index]` with numpy's `IndexError` messages.
    pub fn row(&self, index: usize) -> Result<NdArray, SemanticaError> {
        if self.ndim() == 0 {
            return Err(SemanticaError::Index(
                "too many indices for array: array is 0-dimensional, but 1 were indexed"
                    .to_string(),
            ));
        }
        self.get(index).ok_or_else(|| {
            SemanticaError::Index(format!(
                "index {index} is out of bounds for axis 0 with size {}",
                self.shape[0]
            ))
        })
    }

    /// The sub-arrays along the first axis (`list(array)`).
    pub fn rows(&self) -> Vec<NdArray> {
        if self.ndim() == 0 {
            return Vec::new();
        }
        (0..self.shape[0]).filter_map(|i| self.get(i)).collect()
    }

    /// `array[start:end]` along the first axis (bounds are clamped like Python slices).
    pub fn slice_rows(&self, start: usize, end: usize) -> NdArray {
        if self.ndim() == 0 {
            return self.clone();
        }
        let n = self.shape[0];
        let start = start.min(n);
        let end = end.clamp(start, n);
        let w = self.row_len();
        let mut shape = self.shape.clone();
        shape[0] = end - start;
        match &self.data {
            Data::F32(v) => NdArray {
                shape,
                data: Data::F32(v[start * w..end * w].to_vec()),
            },
            Data::F64(v) => NdArray {
                shape,
                data: Data::F64(v[start * w..end * w].to_vec()),
            },
        }
    }

    /// `array.reshape(shape)` (same number of elements).
    pub fn reshape(&self, shape: &[usize]) -> Result<NdArray, SemanticaError> {
        Self::check_size(shape, self.size())?;
        Ok(NdArray {
            shape: shape.to_vec(),
            data: self.data.clone(),
        })
    }

    /// All elements in row-major order as `f32`.
    pub fn to_vec_f32(&self) -> Vec<f32> {
        match &self.data {
            Data::F32(v) => v.clone(),
            Data::F64(v) => v.iter().map(|x| *x as f32).collect(),
        }
    }

    /// All elements in row-major order as `f64` (exact for `float32` data).
    pub fn to_vec_f64(&self) -> Vec<f64> {
        match &self.data {
            Data::F32(v) => v.iter().map(|x| *x as f64).collect(),
            Data::F64(v) => v.clone(),
        }
    }

    /// The rows of a 2-D array as `f32` vectors (a 1-D array is one row, a 0-D value one
    /// single-element row).
    pub fn to_rows_f32(&self) -> Vec<Vec<f32>> {
        let flat = self.to_vec_f32();
        self.split_rows(flat)
    }

    /// The rows of a 2-D array as `f64` vectors (see [`NdArray::to_rows_f32`]).
    pub fn to_rows_f64(&self) -> Vec<Vec<f64>> {
        let flat = self.to_vec_f64();
        self.split_rows(flat)
    }

    fn split_rows<T: Clone>(&self, flat: Vec<T>) -> Vec<Vec<T>> {
        if self.ndim() < 2 {
            return vec![flat];
        }
        let w = self.shape[1];
        if w == 0 {
            return vec![Vec::new(); self.shape[0]];
        }
        flat.chunks(w).map(|c| c.to_vec()).collect()
    }

    /// The `float32` elements, when the dtype is `float32`.
    pub fn as_f32_slice(&self) -> Option<&[f32]> {
        f32::view(self)
    }

    /// The `float64` elements, when the dtype is `float64`.
    pub fn as_f64_slice(&self) -> Option<&[f64]> {
        f64::view(self)
    }

    /// `float(array)` for a single-element array.
    pub fn item(&self) -> Option<f64> {
        if self.size() == 1 {
            self.to_vec_f64().first().copied()
        } else {
            None
        }
    }

    /// `array.tolist()` as JSON (nested lists of floats; a 0-D value is a bare number).
    /// Non-finite values become `null`, as JSON has no NaN.
    pub fn tolist(&self) -> Value {
        let flat = self.to_vec_f64();
        let num = |x: f64| serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number);
        match self.ndim() {
            0 => num(flat[0]),
            1 => Value::Array(flat.into_iter().map(num).collect()),
            _ => Value::Array(
                self.split_rows(flat)
                    .into_iter()
                    .map(|r| Value::Array(r.into_iter().map(num).collect()))
                    .collect(),
            ),
        }
    }

    // ----- numpy operations used by the embeddings module ---------------------------------

    /// `np.mean(array, axis=0)`.
    pub(crate) fn mean_axis0(&self) -> Result<NdArray, SemanticaError> {
        match self.ndim() {
            0 => Err(SemanticaError::Other {
                module: "numpy.exceptions".to_string(),
                type_name: "AxisError".to_string(),
                message: "axis 0 is out of bounds for array of dimension 0".to_string(),
            }),
            1 => Ok(with_float!(self, |v: T| {
                let n = v.len();
                let mean = if n == 0 {
                    T::NAN
                } else {
                    add_reduce(v).div_count(n)
                };
                T::wrap(Vec::new(), vec![mean])
            })),
            _ => {
                let (rows, cols) = (self.shape[0], self.shape[1]);
                Ok(with_float!(self, |v: T| {
                    let sums = sum_axis0(v, rows, cols);
                    let means = sums
                        .into_iter()
                        .map(|s| if rows == 0 { T::NAN } else { s.div_count(rows) })
                        .collect();
                    T::wrap(vec![cols], means)
                }))
            }
        }
    }

    /// `np.max(array, axis=0)`.
    pub(crate) fn max_axis0(&self) -> Result<NdArray, SemanticaError> {
        if self.ndim() == 0 {
            return Err(SemanticaError::Other {
                module: "numpy.exceptions".to_string(),
                type_name: "AxisError".to_string(),
                message: "axis 0 is out of bounds for array of dimension 0".to_string(),
            });
        }
        if self.shape[0] == 0 {
            return Err(SemanticaError::Value(
                "zero-size array to reduction operation maximum which has no identity"
                    .to_string(),
            ));
        }
        let cols = self.row_len();
        let rows = self.shape[0];
        let out_shape = self.shape[1..].to_vec();
        Ok(with_float!(self, |v: T| {
            let mut out: Vec<T> = v[..cols].to_vec();
            for r in 1..rows {
                for (c, o) in out.iter_mut().enumerate() {
                    *o = maximum(*o, v[r * cols + c]);
                }
            }
            T::wrap(out_shape, out)
        }))
    }

    /// `np.linalg.norm(array)` (Frobenius norm over all elements) in the array's dtype,
    /// returned as `f64`.
    pub(crate) fn norm(&self) -> f64 {
        with_float!(self, |v: T| vector_norm(v).to_f64())
    }
}

impl From<Vec<f32>> for NdArray {
    fn from(values: Vec<f32>) -> Self {
        NdArray::from_vec_f32(values)
    }
}

impl From<Vec<f64>> for NdArray {
    fn from(values: Vec<f64>) -> Self {
        NdArray::from_vec_f64(values)
    }
}

impl TryFrom<Vec<Vec<f32>>> for NdArray {
    type Error = SemanticaError;
    fn try_from(rows: Vec<Vec<f32>>) -> Result<Self, Self::Error> {
        NdArray::from_rows_f32(rows)
    }
}

impl TryFrom<Vec<Vec<f64>>> for NdArray {
    type Error = SemanticaError;
    fn try_from(rows: Vec<Vec<f64>>) -> Result<Self, Self::Error> {
        NdArray::from_rows_f64(rows)
    }
}

impl Serialize for NdArray {
    /// Serialises as `array.tolist()`.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.tolist().serialize(serializer)
    }
}

impl fmt::Debug for NdArray {
    /// `array([...], dtype=float32)`-style rendering.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "array({}, dtype={})", self.tolist(), self.dtype())
    }
}
