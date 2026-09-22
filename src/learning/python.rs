use super::*;
use numpy::{IntoPyArray, PyArray1, PyArray2, PyReadonlyArray1, PyReadonlyArray2, ndarray};
use pyo3::{
    exceptions::PyValueError,
    prelude::*,
    pybacked::PyBackedBytes,
    types::{PyBytes, PyList, PyTuple},
};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};
use std::path::Path;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{
    filter::filter_fn,
    fmt::{FmtContext, FormatEvent, FormatFields, format::Writer},
    prelude::*,
    registry::LookupSpan,
};

#[derive(Clone)]
struct GlogFormat {
    role: String,
}

fn native_thread_id() -> u64 {
    #[cfg(target_os = "macos")]
    {
        let mut id = 0;
        unsafe { libc::pthread_threadid_np(0, &mut id) };
        id
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&std::thread::current().id(), &mut hash);
        hash.finish()
    }
}

impl<S, N> FormatEvent<S, N> for GlogFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        context: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();
        let severity = match *metadata.level() {
            Level::TRACE => 'D',
            Level::DEBUG => 'D',
            Level::INFO => 'I',
            Level::WARN => 'W',
            Level::ERROR => 'E',
        };
        let mut now = unsafe { std::mem::zeroed::<libc::timeval>() };
        let mut local = unsafe { std::mem::zeroed::<libc::tm>() };
        unsafe {
            libc::gettimeofday(&mut now, std::ptr::null_mut());
            libc::localtime_r(&now.tv_sec, &mut local);
        }
        let thread = std::thread::current();
        let rayon_name = rayon::current_thread_index().map(|index| format!("rayon-{index}"));
        let thread_name = rayon_name.as_deref().or(thread.name()).unwrap_or("unnamed");
        let file = metadata
            .file()
            .and_then(|file| Path::new(file).file_name())
            .and_then(|file| file.to_str())
            .unwrap_or("unknown");
        write!(
            writer,
            "{severity}{:02}{:02} {:02}:{:02}:{:02}.{:06} {:06} {:08} {:<12} {:<16} {:>20}:{:05}] ",
            local.tm_mon + 1,
            local.tm_mday,
            local.tm_hour,
            local.tm_min,
            local.tm_sec,
            now.tv_usec,
            std::process::id(),
            native_thread_id(),
            self.role,
            thread_name,
            file,
            metadata.line().unwrap_or(0),
        )?;
        context
            .field_format()
            .format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

fn level_enabled(level: &Level, configured: Level) -> bool {
    let rank = |level: &Level| match *level {
        Level::ERROR => 0,
        Level::WARN => 1,
        Level::INFO => 2,
        Level::DEBUG => 3,
        Level::TRACE => 4,
    };
    rank(level) <= rank(&configured)
}

#[pyfunction]
#[allow(clippy::type_complexity)]
fn model_schema() -> (
    u32,
    u32,
    usize,
    usize,
    Vec<usize>,
    Vec<(&'static str, usize, usize, usize, usize)>,
    Vec<&'static str>,
    Vec<(&'static str, u32)>,
) {
    (
        VERSION,
        VALUE_MODEL_VERSION,
        TERMINAL_CATEGORIES,
        TERMINAL_CATEGORIES - 11,
        vec![ACTION_U, ACTION_S, ACTION_C, ACTION_F],
        DOMAIN_NAMES
            .into_iter()
            .zip(DOMAIN_WIDTHS)
            .map(|(name, (u, s, c, f))| (name, u, s, c, f))
            .collect(),
        SEMANTIC_NAMES.to_vec(),
        POSITION_NAMES.into_iter().zip(POSITION_CAPS).collect(),
    )
}

#[pyfunction]
fn configure_logging(role: String, level: &str) -> PyResult<()> {
    let configured = match level.to_ascii_uppercase().as_str() {
        "ERROR" => Level::ERROR,
        "WARNING" | "WARN" => Level::WARN,
        "INFO" => Level::INFO,
        "DEBUG" => Level::DEBUG,
        other => return Err(PyValueError::new_err(format!("invalid log level {other}"))),
    };
    let stdout = tracing_subscriber::fmt::layer()
        .event_format(GlogFormat { role: role.clone() })
        .with_ansi(false)
        .with_writer(std::io::stdout)
        .with_filter(filter_fn(move |metadata| {
            level_enabled(metadata.level(), configured)
                && matches!(*metadata.level(), Level::TRACE | Level::DEBUG | Level::INFO)
        }));
    let stderr = tracing_subscriber::fmt::layer()
        .event_format(GlogFormat { role })
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .with_filter(filter_fn(move |metadata| {
            level_enabled(metadata.level(), configured)
                && matches!(*metadata.level(), Level::WARN | Level::ERROR)
        }));
    tracing_subscriber::registry()
        .with(stdout)
        .with(stderr)
        .try_init()
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

struct FastHasher(u64);

impl Default for FastHasher {
    fn default() -> Self {
        Self(0x517cc1b727220a95)
    }
}

impl Hasher for FastHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            self.write_u64(u64::from_ne_bytes(chunk.try_into().unwrap()));
        }
        for &byte in chunks.remainder() {
            self.write_u8(byte);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u64(value as u64);
    }

    fn write_u32(&mut self, value: u32) {
        self.write_u64(value as u64);
    }

    fn write_u64(&mut self, value: u64) {
        self.0 ^= value.wrapping_add(0x9e3779b97f4a7c15);
        self.0 = self.0.rotate_left(27).wrapping_mul(0x3c79ac492ba7b653);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

fn hash_words(hash: &mut FastHasher, words: &[u32]) {
    hash.write_usize(words.len());
    let mut pairs = words.chunks_exact(2);
    for pair in &mut pairs {
        hash.write_u64(pair[0] as u64 | (pair[1] as u64) << 32);
    }
    if let Some(&value) = pairs.remainder().first() {
        hash.write_u32(value);
    }
}

fn compress_words(values: &[u32]) -> Vec<u8> {
    let bitmap = values.len().div_ceil(8);
    let mut output = vec![0; 4 + bitmap];
    output[..4].copy_from_slice(&(values.len() as u32).to_le_bytes());
    for (index, &value) in values.iter().enumerate() {
        if value != 0 {
            output[4 + index / 8] |= 1 << (index % 8);
            output.extend_from_slice(&value.to_le_bytes());
        }
    }
    output
}

#[cfg(target_os = "macos")]
fn compress_bytes(input: &[u8]) -> Vec<u8> {
    #[link(name = "compression")]
    unsafe extern "C" {
        fn compression_encode_buffer(
            output: *mut u8,
            output_size: usize,
            input: *const u8,
            input_size: usize,
            scratch: *mut std::ffi::c_void,
            algorithm: u32,
        ) -> usize;
    }
    let mut output = vec![0; input.len() + input.len() / 255 + 16];
    let size = unsafe {
        compression_encode_buffer(
            output.as_mut_ptr(),
            output.len(),
            input.as_ptr(),
            input.len(),
            std::ptr::null_mut(),
            0x100,
        )
    };
    assert!(size > 0);
    output.truncate(size);
    output
}

#[cfg(target_os = "macos")]
fn decompress_bytes(input: &[u8], size: usize) -> PyResult<Vec<u8>> {
    #[link(name = "compression")]
    unsafe extern "C" {
        fn compression_decode_buffer(
            output: *mut u8,
            output_size: usize,
            input: *const u8,
            input_size: usize,
            scratch: *mut std::ffi::c_void,
            algorithm: u32,
        ) -> usize;
    }
    let mut output = vec![0; size];
    let written = unsafe {
        compression_decode_buffer(
            output.as_mut_ptr(),
            output.len(),
            input.as_ptr(),
            input.len(),
            std::ptr::null_mut(),
            0x100,
        )
    };
    if written != size {
        return Err(PyValueError::new_err("invalid compressed observation"));
    }
    Ok(output)
}

fn compact_packed_observation(row: &ObservationV56) -> Vec<u8> {
    compact_packed_observation_known(row, None)
}

fn cached_packed_metadata(row: &ObservationV56) -> Vec<u8> {
    let mut output = vec![0; 32 + 4 * DOMAIN_WIDTHS.len()];
    output[..4].copy_from_slice(b"SP68");
    output[4] = row.character;
    output[16..20].copy_from_slice(&(row.candidates.len() as u32).to_le_bytes());
    output[20..24].copy_from_slice(
        &(row
            .candidates
            .iter()
            .filter(|candidate| candidate.legal)
            .count() as u32)
            .to_le_bytes(),
    );
    output
}

fn compact_packed_observation_known(row: &ObservationV56, digest: Option<u64>) -> Vec<u8> {
    compact_packed_observation_and_digest(row, digest).0
}

fn compact_packed_observation_and_digest(
    row: &ObservationV56,
    digest: Option<u64>,
) -> (Vec<u8>, u64) {
    let (globals, counts, exact, actions, digest) = packed_observation_known(row, digest);
    let exact = compress_words(&exact);
    let exact_len = exact.len();
    let width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
    let action_count = actions.len() / width;
    let legal_count = actions
        .chunks_exact(width)
        .filter(|row| row[width - 1] != 0)
        .count();
    let mut payload = exact;
    actions
        .iter()
        .for_each(|value| payload.extend(value.to_le_bytes()));
    #[cfg(target_os = "macos")]
    let payload = compress_bytes(&payload);
    let mut output = Vec::with_capacity(32 + 4 * (globals.len() + counts.len()) + payload.len());
    output.extend(if cfg!(target_os = "macos") {
        b"SP68"
    } else {
        b"SP67"
    });
    output.push(row.character);
    output.extend([0; 3]);
    output.extend(digest.to_le_bytes());
    output.extend((action_count as u32).to_le_bytes());
    output.extend((legal_count as u32).to_le_bytes());
    output.extend((exact_len as u32).to_le_bytes());
    output.extend(0u32.to_le_bytes());
    globals
        .iter()
        .for_each(|value| output.extend(value.to_bits().to_le_bytes()));
    counts
        .iter()
        .for_each(|value| output.extend(value.to_le_bytes()));
    output.extend(payload);
    (output, digest)
}

fn decompress_words(input: &[u8], expected: usize) -> PyResult<Vec<u32>> {
    if input.len() < 4 {
        return Err(PyValueError::new_err("invalid packed word stream"));
    }
    let words = u32::from_le_bytes(input[..4].try_into().unwrap()) as usize;
    let bitmap = words.div_ceil(8);
    if words != expected || input.len() < 4 + bitmap {
        return Err(PyValueError::new_err("invalid packed word stream"));
    }
    let mut output = vec![0; words];
    let mut offset = 4 + bitmap;
    for (index, value) in output.iter_mut().enumerate() {
        if input[4 + index / 8] & (1 << (index % 8)) != 0 {
            let word = input
                .get(offset..offset + 4)
                .ok_or_else(|| PyValueError::new_err("invalid packed word stream"))?;
            *value = u32::from_le_bytes(word.try_into().unwrap());
            offset += 4;
        }
    }
    if offset != input.len() {
        return Err(PyValueError::new_err("invalid packed word stream"));
    }
    Ok(output)
}

struct PackedData {
    character: u8,
    globals: Vec<f32>,
    counts: Vec<u32>,
    exact: Vec<u32>,
    actions: Vec<u32>,
    digest: u64,
}

fn compact_data(input: &[u8]) -> PyResult<PackedData> {
    let width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
    let fixed = 32 + 4 * (PUBLIC_GLOBALS + DOMAIN_WIDTHS.len());
    if input.len() < fixed || !matches!(&input[..4], b"SP67" | b"SP68") {
        return Err(PyValueError::new_err("invalid compact observation"));
    }
    let digest = u64::from_le_bytes(input[8..16].try_into().unwrap());
    let actions = u32::from_le_bytes(input[16..20].try_into().unwrap()) as usize;
    let exact_bytes = u32::from_le_bytes(input[24..28].try_into().unwrap()) as usize;
    let mut offset = 32;
    let globals = input[offset..offset + 4 * PUBLIC_GLOBALS]
        .chunks_exact(4)
        .map(|value| f32::from_bits(u32::from_le_bytes(value.try_into().unwrap())))
        .collect::<Vec<_>>();
    offset += 4 * PUBLIC_GLOBALS;
    let counts = input[offset..offset + 4 * DOMAIN_WIDTHS.len()]
        .chunks_exact(4)
        .map(|value| u32::from_le_bytes(value.try_into().unwrap()))
        .collect::<Vec<_>>();
    offset += 4 * DOMAIN_WIDTHS.len();
    let action_bytes = actions * width * 4;
    #[cfg(target_os = "macos")]
    let payload = if &input[..4] == b"SP68" {
        decompress_bytes(&input[offset..], exact_bytes + action_bytes)?
    } else {
        input[offset..].to_vec()
    };
    #[cfg(not(target_os = "macos"))]
    let payload = input[offset..].to_vec();
    if payload.len() != exact_bytes + action_bytes {
        return Err(PyValueError::new_err("invalid compact observation length"));
    }
    let expected = counts
        .iter()
        .zip(DOMAIN_WIDTHS)
        .map(|(&count, (u, s, c, f))| count as usize * (u + s + c + f + 1))
        .sum();
    let exact = decompress_words(&payload[..exact_bytes], expected)?;
    let actions = payload[exact_bytes..]
        .chunks_exact(4)
        .map(|value| u32::from_le_bytes(value.try_into().unwrap()))
        .collect();
    Ok(PackedData {
        character: input[4],
        globals,
        counts,
        exact,
        actions,
        digest,
    })
}

fn packed_data(row: &Bound<'_, PyAny>) -> PyResult<PackedData> {
    if let Ok(row) = row.downcast::<PyBytes>() {
        return compact_data(row.as_bytes());
    }
    let row = row.downcast::<PyTuple>()?;
    let character = row.get_item(0)?.extract()?;
    let globals = row
        .get_item(1)?
        .extract::<PyReadonlyArray1<'_, f32>>()?
        .as_slice()?
        .to_vec();
    let counts = row
        .get_item(2)?
        .extract::<PyReadonlyArray1<'_, u32>>()?
        .as_slice()?
        .to_vec();
    let expected = counts
        .iter()
        .zip(DOMAIN_WIDTHS)
        .map(|(&count, (u, s, c, f))| count as usize * (u + s + c + f + 1))
        .sum();
    let exact_item = row.get_item(3)?;
    let exact = if let Ok(exact) = exact_item.downcast::<PyBytes>() {
        decompress_words(exact.as_bytes(), expected)?
    } else {
        exact_item
            .extract::<PyReadonlyArray1<'_, u32>>()?
            .as_slice()?
            .to_vec()
    };
    let actions = row
        .get_item(4)?
        .extract::<PyReadonlyArray2<'_, u32>>()?
        .as_slice()?
        .to_vec();
    Ok(PackedData {
        character,
        globals,
        counts,
        exact,
        actions,
        digest: row.get_item(5)?.extract()?,
    })
}

#[pyfunction]
fn unique_rows<'py>(
    py: Python<'py>,
    values: PyReadonlyArray2<'py, u32>,
) -> (
    Bound<'py, numpy::PyArray1<i64>>,
    Bound<'py, numpy::PyArray1<i64>>,
) {
    let values = values.as_array();
    if values.nrows() == 0 {
        return (Vec::new().into_pyarray(py), Vec::new().into_pyarray(py));
    }
    let columns = values.ncols();
    let flat = values.as_slice().unwrap();
    let mut unique =
        FastMap::with_capacity_and_hasher(values.nrows(), BuildHasherDefault::default());
    let mut first = Vec::new();
    let mut inverse = Vec::with_capacity(values.nrows());
    for (index, row) in flat.chunks(columns).enumerate() {
        let next = unique.len();
        let value = *unique.entry(row).or_insert_with(|| {
            first.push(index as i64);
            next
        });
        inverse.push(value as i64);
    }
    (first.into_pyarray(py), inverse.into_pyarray(py))
}

#[pyfunction]
fn unique_feature_rows<'py>(
    py: Python<'py>,
    semantic: PyReadonlyArray2<'py, u32>,
    numeric: PyReadonlyArray2<'py, u32>,
) -> PyResult<(
    Bound<'py, numpy::PyArray1<i64>>,
    Bound<'py, numpy::PyArray1<i64>>,
)> {
    let semantic = semantic.as_array();
    let numeric = numeric.as_array();
    if semantic.nrows() != numeric.nrows() {
        return Err(PyValueError::new_err("feature row count mismatch"));
    }
    if semantic.nrows() == 0 {
        return Ok((Vec::new().into_pyarray(py), Vec::new().into_pyarray(py)));
    }
    let rows = semantic.nrows();
    let semantic = semantic
        .as_slice()
        .ok_or_else(|| PyValueError::new_err("semantic rows must be contiguous"))?;
    let numeric = numeric
        .as_slice()
        .ok_or_else(|| PyValueError::new_err("numeric rows must be contiguous"))?;
    let semantic_width = semantic.len() / rows;
    let numeric_width = numeric.len() / rows;
    let hashes = py.allow_threads(|| {
        (0..rows)
            .into_par_iter()
            .map(|index| {
                let mut hash = FastHasher::default();
                hash_words(
                    &mut hash,
                    &semantic[index * semantic_width..(index + 1) * semantic_width],
                );
                hash_words(
                    &mut hash,
                    &numeric[index * numeric_width..(index + 1) * numeric_width],
                );
                hash.finish()
            })
            .collect::<Vec<_>>()
    });
    let mut unique = FastMap::<u64, Vec<(usize, usize)>>::with_capacity_and_hasher(
        rows,
        BuildHasherDefault::default(),
    );
    let mut first = Vec::new();
    let mut inverse = Vec::with_capacity(rows);
    for (index, hash) in hashes.into_iter().enumerate() {
        let semantic_row = &semantic[index * semantic_width..(index + 1) * semantic_width];
        let numeric_row = &numeric[index * numeric_width..(index + 1) * numeric_width];
        let matches = |source: usize| {
            semantic_row == &semantic[source * semantic_width..(source + 1) * semantic_width]
                && numeric_row == &numeric[source * numeric_width..(source + 1) * numeric_width]
        };
        let entries = unique.entry(hash).or_default();
        let value = entries
            .iter()
            .find_map(|&(source, value)| matches(source).then_some(value))
            .unwrap_or_else(|| {
                let value = first.len();
                first.push(index as i64);
                entries.push((index, value));
                value
            });
        inverse.push(value as i64);
    }
    Ok((first.into_pyarray(py), inverse.into_pyarray(py)))
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn unique_graphs<'py>(
    py: Python<'py>,
    node_u: PyReadonlyArray2<'py, u32>,
    node_c: PyReadonlyArray2<'py, u32>,
    node_f: PyReadonlyArray2<'py, f32>,
    node_source: PyReadonlyArray1<'py, i32>,
    node_offsets: PyReadonlyArray1<'py, i64>,
    edge_u: PyReadonlyArray2<'py, u32>,
    edge_c: PyReadonlyArray2<'py, u32>,
    edge_f: PyReadonlyArray2<'py, f32>,
    edge_source: PyReadonlyArray1<'py, i32>,
    edge_offsets: PyReadonlyArray1<'py, i64>,
) -> PyResult<(
    Bound<'py, numpy::PyArray1<i64>>,
    Bound<'py, numpy::PyArray1<i64>>,
)> {
    let (node_u, node_c, node_f) = (node_u.as_array(), node_c.as_array(), node_f.as_array());
    let (edge_u, edge_c, edge_f) = (edge_u.as_array(), edge_c.as_array(), edge_f.as_array());
    let node_source = node_source.as_slice()?;
    let edge_source = edge_source.as_slice()?;
    let node_offsets = node_offsets.as_slice()?;
    let edge_offsets = edge_offsets.as_slice()?;
    if node_offsets.len() != edge_offsets.len() {
        return Err(PyValueError::new_err("graph row count mismatch"));
    }
    let rows = node_offsets.len().saturating_sub(1);
    let keys = py.allow_threads(|| {
        (0..rows)
            .into_par_iter()
            .map(|row| {
                let node_range = node_offsets[row] as usize..node_offsets[row + 1] as usize;
                let edge_range = edge_offsets[row] as usize..edge_offsets[row + 1] as usize;
                let mut key = Vec::with_capacity(
                    2 + node_range.len() * (node_u.ncols() + node_c.ncols() + node_f.ncols())
                        + edge_range.len() * (edge_u.ncols() + edge_c.ncols() + edge_f.ncols()),
                );
                key.push(node_range.len() as u32);
                for &source in &node_source[node_range] {
                    let source = source as usize;
                    key.extend(node_u.row(source));
                    key.extend(node_c.row(source));
                    key.extend(node_f.row(source).iter().map(|value| value.to_bits()));
                }
                key.push(edge_range.len() as u32);
                for &source in &edge_source[edge_range] {
                    let source = source as usize;
                    key.extend(edge_u.row(source));
                    key.extend(edge_c.row(source));
                    key.extend(edge_f.row(source).iter().map(|value| value.to_bits()));
                }
                let mut hash = FastHasher::default();
                hash_words(&mut hash, &key);
                (hash.finish(), key)
            })
            .collect::<Vec<_>>()
    });
    let mut unique = FastMap::<u64, Vec<(usize, usize)>>::with_capacity_and_hasher(
        rows,
        BuildHasherDefault::default(),
    );
    let mut first = Vec::new();
    let mut inverse = Vec::with_capacity(rows);
    for (row, (hash, key)) in keys.iter().enumerate() {
        let entries = unique.entry(*hash).or_default();
        let value = entries
            .iter()
            .find_map(|&(source, value)| (key == &keys[source].1).then_some(value))
            .unwrap_or_else(|| {
                let value = first.len();
                first.push(row as i64);
                entries.push((row, value));
                value
            });
        inverse.push(value as i64);
    }
    Ok((first.into_pyarray(py), inverse.into_pyarray(py)))
}

fn check_action_features(unsigned: &[u32], signed: &[i32], numeric: &[f32]) -> PyResult<()> {
    let unsigned: &[u32; ACTION_U] = unsigned
        .try_into()
        .map_err(|_| PyValueError::new_err("invalid action unsigned width"))?;
    let signed: &[i32; ACTION_S] = signed
        .try_into()
        .map_err(|_| PyValueError::new_err("invalid action signed width"))?;
    let expected = action_features(unsigned, signed);
    if numeric.len() != ACTION_F
        || numeric
            .iter()
            .zip(expected)
            .any(|(actual, expected)| actual.to_bits() != expected.to_bits())
    {
        return Err(PyValueError::new_err("action auxiliary mismatch"));
    }
    Ok(())
}

#[pyfunction]
fn validate_action_features(
    unsigned: PyReadonlyArray2<'_, u32>,
    signed: PyReadonlyArray2<'_, i32>,
    numeric: PyReadonlyArray2<'_, f32>,
) -> PyResult<()> {
    let (unsigned, signed, numeric) = (unsigned.as_array(), signed.as_array(), numeric.as_array());
    if unsigned.nrows() != signed.nrows() || unsigned.nrows() != numeric.nrows() {
        return Err(PyValueError::new_err("action row count mismatch"));
    }
    for row in 0..unsigned.nrows() {
        check_action_features(
            unsigned.row(row).as_slice().unwrap(),
            signed.row(row).as_slice().unwrap(),
            numeric.row(row).as_slice().unwrap(),
        )?;
    }
    Ok(())
}

#[pyfunction]
fn compress_packed_observations<'py>(
    py: Python<'py>,
    rows: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyTuple>> {
    let mut output = Vec::with_capacity(rows.len());
    for item in rows.iter() {
        let row = item.downcast::<PyTuple>()?;
        let exact = row.get_item(3)?.extract::<PyReadonlyArray1<'_, u32>>()?;
        let compressed = compress_words(exact.as_slice()?);
        output.push(PyTuple::new(
            py,
            [
                row.get_item(0)?,
                row.get_item(1)?,
                row.get_item(2)?,
                PyBytes::new(py, &compressed).into_any(),
                row.get_item(4)?,
                row.get_item(5)?,
            ],
        )?);
    }
    PyTuple::new(py, output)
}

#[pyfunction]
fn validate_packed_observation(
    character: u8,
    globals: PyReadonlyArray1<'_, f32>,
    counts: PyReadonlyArray1<'_, u32>,
    exact: &Bound<'_, PyAny>,
    actions: PyReadonlyArray2<'_, u32>,
    digest: u64,
) -> PyResult<()> {
    let globals = globals
        .as_slice()
        .map_err(|_| PyValueError::new_err("noncontiguous public globals"))?;
    let counts = counts
        .as_slice()
        .map_err(|_| PyValueError::new_err("noncontiguous domain counts"))?;
    let expected = counts
        .iter()
        .zip(DOMAIN_WIDTHS)
        .map(|(&count, (u, s, c, f))| count as usize * (u + s + c + f + 1))
        .sum::<usize>();
    let exact_array: PyReadonlyArray1<'_, u32>;
    let exact_owned: Vec<u32>;
    let exact = if let Ok(bytes) = exact.downcast::<PyBytes>() {
        exact_owned = decompress_words(bytes.as_bytes(), expected)?;
        exact_owned.as_slice()
    } else {
        exact_array = exact.extract()?;
        exact_array
            .as_slice()
            .map_err(|_| PyValueError::new_err("noncontiguous domain rows"))?
    };
    let actions = actions.as_array();
    let action_width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
    if globals.len() != PUBLIC_GLOBALS
        || !globals.iter().all(|value| value.is_finite())
        || counts.len() != DOMAIN_WIDTHS.len()
        || exact.len() != expected
        || actions.ncols() != action_width
    {
        return Err(PyValueError::new_err("invalid packed observation schema"));
    }
    let action_words = actions
        .as_slice()
        .ok_or_else(|| PyValueError::new_err("noncontiguous action rows"))?;
    if packed_observation_digest(character, globals, counts, exact, action_words) != digest {
        return Err(PyValueError::new_err(
            "observation auxiliary digest mismatch",
        ));
    }
    for row in actions.rows() {
        let row = row.as_slice().unwrap();
        let signed = row[ACTION_U..ACTION_U + ACTION_S]
            .iter()
            .map(|&value| value as i32)
            .collect::<Vec<_>>();
        let numeric = row
            [ACTION_U + ACTION_S + ACTION_C..ACTION_U + ACTION_S + ACTION_C + ACTION_F]
            .iter()
            .map(|&value| f32::from_bits(value))
            .collect::<Vec<_>>();
        check_action_features(&row[..ACTION_U], &signed, &numeric)?;
        if row[action_width - 1] > 1 {
            return Err(PyValueError::new_err("invalid action legality"));
        }
    }
    Ok(())
}

#[pyfunction]
fn validate_compact_observation(row: &Bound<'_, PyBytes>) -> PyResult<()> {
    let row = compact_data(row.as_bytes())?;
    let width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
    if packed_observation_digest(
        row.character,
        &row.globals,
        &row.counts,
        &row.exact,
        &row.actions,
    ) != row.digest
    {
        return Err(PyValueError::new_err(
            "observation auxiliary digest mismatch",
        ));
    }
    for action in row.actions.chunks_exact(width) {
        let signed = action[ACTION_U..ACTION_U + ACTION_S]
            .iter()
            .map(|&value| value as i32)
            .collect::<Vec<_>>();
        let numeric = action
            [ACTION_U + ACTION_S + ACTION_C..ACTION_U + ACTION_S + ACTION_C + ACTION_F]
            .iter()
            .map(|&value| f32::from_bits(value))
            .collect::<Vec<_>>();
        check_action_features(&action[..ACTION_U], &signed, &numeric)?;
        if action[width - 1] > 1 {
            return Err(PyValueError::new_err("invalid action legality"));
        }
    }
    Ok(())
}

type DomainArrays<'py> = (
    Bound<'py, PyArray2<u32>>,
    Bound<'py, PyArray2<i32>>,
    Bound<'py, PyArray2<u32>>,
    Bound<'py, PyArray2<f32>>,
    Bound<'py, PyArray1<i32>>,
    Bound<'py, PyArray1<i32>>,
    Bound<'py, PyArray1<i64>>,
    Bound<'py, PyArray1<i64>>,
);

type DomainVectors = (
    Vec<u32>,
    Vec<i32>,
    Vec<u32>,
    Vec<f32>,
    Vec<i32>,
    Vec<i32>,
    Vec<i64>,
    Vec<i64>,
);

fn unpack_domain(
    packed: &[PackedData],
    action_offsets: &[usize],
    domain: usize,
    rows: usize,
) -> DomainVectors {
    let (u, s, c, f) = DOMAIN_WIDTHS[domain];
    let width = u + s + c + f + 1;
    let mut unsigned = Vec::with_capacity(rows * u);
    let mut signed = Vec::with_capacity(rows * s);
    let mut semantic = Vec::with_capacity(rows * c);
    let mut numeric = Vec::<f32>::with_capacity(rows * f);
    let mut row_index = Vec::with_capacity(rows);
    let mut scope = Vec::with_capacity(rows);
    let mut first = Vec::new();
    let mut inverse = Vec::with_capacity(rows);
    let mut unique = FastMap::<u64, Vec<(usize, usize)>>::with_capacity_and_hasher(
        rows,
        BuildHasherDefault::default(),
    );
    for (batch, row) in packed.iter().enumerate() {
        let offset = row
            .counts
            .iter()
            .zip(DOMAIN_WIDTHS)
            .take(domain)
            .map(|(&count, (u, s, c, f))| count as usize * (u + s + c + f + 1))
            .sum::<usize>();
        let end = offset + row.counts[domain] as usize * width;
        for record in row.exact[offset..end].chunks_exact(width) {
            let semantic_words = &record[u + s..u + s + c];
            let numeric_words = &record[u + s + c..u + s + c + f];
            let mut hash = FastHasher::default();
            hash_words(&mut hash, semantic_words);
            hash_words(&mut hash, numeric_words);
            let entries = unique.entry(hash.finish()).or_default();
            let value = entries
                .iter()
                .find_map(|&(source, value)| {
                    (semantic_words == &semantic[source * c..(source + 1) * c]
                        && numeric_words
                            .iter()
                            .copied()
                            .eq(numeric[source * f..(source + 1) * f]
                                .iter()
                                .map(|value| value.to_bits())))
                    .then_some(value)
                })
                .unwrap_or_else(|| {
                    let value = first.len();
                    first.push(row_index.len() as i64);
                    entries.push((row_index.len(), value));
                    value
                });
            inverse.push(value as i64);
            unsigned.extend_from_slice(&record[..u]);
            signed.extend(record[u..u + s].iter().map(|&value| value as i32));
            semantic.extend_from_slice(semantic_words);
            numeric.extend(numeric_words.iter().map(|&value| f32::from_bits(value)));
            row_index.push(batch as i32);
            let value = record[width - 1] as i32;
            scope.push(if value < 0 {
                value
            } else {
                value + action_offsets[batch] as i32
            });
        }
    }
    (
        unsigned, signed, semantic, numeric, row_index, scope, first, inverse,
    )
}

#[pyfunction]
fn unpack_packed_observations<'py>(
    py: Python<'py>,
    rows: &Bound<'py, PyList>,
) -> PyResult<(
    Bound<'py, PyArray1<u8>>,
    Bound<'py, PyArray2<f32>>,
    Vec<DomainArrays<'py>>,
    (
        Bound<'py, PyArray2<u32>>,
        Bound<'py, PyArray2<i32>>,
        Bound<'py, PyArray2<u32>>,
        Bound<'py, PyArray2<f32>>,
    ),
    Bound<'py, PyArray1<i32>>,
    Bound<'py, PyArray1<i32>>,
    Bound<'py, PyArray2<bool>>,
)> {
    let batch = rows.len();
    let compact = rows
        .iter()
        .map(|row| row.extract::<PyBackedBytes>())
        .collect::<PyResult<Vec<_>>>();
    let packed = match compact {
        Ok(rows) => py.allow_threads(|| {
            rows.par_iter()
                .map(|row| compact_data(row))
                .collect::<PyResult<Vec<_>>>()
        })?,
        Err(_) => rows
            .iter()
            .map(|row| packed_data(&row))
            .collect::<PyResult<Vec<_>>>()?,
    };
    let mut characters = Vec::with_capacity(batch);
    let mut globals = Vec::with_capacity(batch * PUBLIC_GLOBALS);
    let mut lengths = Vec::with_capacity(batch);
    let mut domain_rows = [0; DOMAIN_WIDTHS.len()];
    for row in &packed {
        characters.push(row.character);
        if row.globals.len() != PUBLIC_GLOBALS {
            return Err(PyValueError::new_err("invalid packed globals"));
        }
        globals.extend_from_slice(&row.globals);
        let width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
        if row.actions.len() % width != 0 {
            return Err(PyValueError::new_err("invalid packed action width"));
        }
        lengths.push(row.actions.len() / width);
        if row.counts.len() != DOMAIN_WIDTHS.len() {
            return Err(PyValueError::new_err("invalid packed domain count"));
        }
        let expected = row
            .counts
            .iter()
            .zip(DOMAIN_WIDTHS)
            .map(|(&count, (u, s, c, f))| count as usize * (u + s + c + f + 1))
            .sum::<usize>();
        if row.exact.len() != expected {
            return Err(PyValueError::new_err("invalid packed domain rows"));
        }
        for (total, &count) in domain_rows.iter_mut().zip(&row.counts) {
            *total += count as usize;
        }
    }
    let max_actions = lengths.iter().copied().max().unwrap_or(1).max(1);
    let total_actions: usize = lengths.iter().sum();
    let mut offset = 0;
    let action_offsets = lengths
        .iter()
        .map(|&length| {
            let current = offset;
            offset += length;
            current
        })
        .collect::<Vec<_>>();
    let mut domains = py.allow_threads(|| {
        (0..DOMAIN_WIDTHS.len())
            .into_par_iter()
            .map(|domain| unpack_domain(&packed, &action_offsets, domain, domain_rows[domain]))
            .collect::<Vec<_>>()
    });
    let mut action_u = Vec::with_capacity(total_actions * ACTION_U);
    let mut action_s = Vec::with_capacity(total_actions * ACTION_S);
    let mut action_c = Vec::with_capacity(total_actions * ACTION_C);
    let mut action_f = Vec::with_capacity(total_actions * ACTION_F);
    let mut action_row = Vec::with_capacity(total_actions);
    let mut action_position = Vec::with_capacity(total_actions);
    let mut legal = vec![false; batch * max_actions];
    for (batch_index, row) in packed.iter().enumerate() {
        let action_width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
        for (position, action) in row.actions.chunks_exact(action_width).enumerate() {
            action_u.extend_from_slice(&action[..ACTION_U]);
            action_s.extend(
                action[ACTION_U..ACTION_U + ACTION_S]
                    .iter()
                    .map(|&value| value as i32),
            );
            action_c
                .extend_from_slice(&action[ACTION_U + ACTION_S..ACTION_U + ACTION_S + ACTION_C]);
            action_f.extend(
                action[ACTION_U + ACTION_S + ACTION_C..ACTION_U + ACTION_S + ACTION_C + ACTION_F]
                    .iter()
                    .map(|&value| f32::from_bits(value)),
            );
            action_row.push(batch_index as i32);
            action_position.push(position as i32);
            legal[batch_index * max_actions + position] =
                action[ACTION_U + ACTION_S + ACTION_C + ACTION_F] != 0;
        }
    }
    let domains = DOMAIN_WIDTHS
        .iter()
        .enumerate()
        .map(|(domain, &(u, s, c, f))| {
            let (unsigned, signed, semantic, numeric, row_index, scope, first, inverse) =
                std::mem::take(&mut domains[domain]);
            let rows = row_index.len();
            (
                ndarray::Array2::from_shape_vec((rows, u), unsigned)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array2::from_shape_vec((rows, s), signed)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array2::from_shape_vec((rows, c), semantic)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array2::from_shape_vec((rows, f), numeric)
                    .unwrap()
                    .into_pyarray(py),
                row_index.into_pyarray(py),
                scope.into_pyarray(py),
                first.into_pyarray(py),
                inverse.into_pyarray(py),
            )
        })
        .collect();
    Ok((
        characters.into_pyarray(py),
        ndarray::Array2::from_shape_vec((batch, PUBLIC_GLOBALS), globals)
            .unwrap()
            .into_pyarray(py),
        domains,
        (
            ndarray::Array2::from_shape_vec((total_actions, ACTION_U), action_u)
                .unwrap()
                .into_pyarray(py),
            ndarray::Array2::from_shape_vec((total_actions, ACTION_S), action_s)
                .unwrap()
                .into_pyarray(py),
            ndarray::Array2::from_shape_vec((total_actions, ACTION_C), action_c)
                .unwrap()
                .into_pyarray(py),
            ndarray::Array2::from_shape_vec((total_actions, ACTION_F), action_f)
                .unwrap()
                .into_pyarray(py),
        ),
        action_row.into_pyarray(py),
        action_position.into_pyarray(py),
        ndarray::Array2::from_shape_vec((batch, max_actions), legal)
            .unwrap()
            .into_pyarray(py),
    ))
}

#[pyclass]
struct Batch {
    content: Content,
    layout: Layout,
    games: Vec<Game>,
    actions: Vec<Vec<Action>>,
    plans: Vec<Vec<Action>>,
    starts: Vec<Game>,
    root_ids: Vec<usize>,
    archive: Vec<Vec<Game>>,
    archive_seen: Vec<u64>,
    first_archive: Vec<Game>,
    random: u64,
    next_seed: u64,
    seed_stride: u64,
    character: Option<Id>,
    ascension: u8,
    teacher_width: usize,
    teacher_turns: usize,
    first_teacher: Option<(usize, usize)>,
    training_strength: i16,
    training_dexterity: i16,
    potential_weights: [f32; POTENTIAL_WEIGHT_COUNT],
    resample_archive: bool,
    archive_depth: usize,
    policy: Option<ValueModel>,
    searched_turns: Vec<Option<u16>>,
}

struct SearchEdge {
    row: CandidateRow,
    candidate: usize,
    occurrence: usize,
    kind_occurrence: usize,
    prior: f32,
    behavior: f32,
    rank: usize,
    visits: u32,
    value_sum: f32,
    children: Vec<(usize, u32)>,
    terminal_visits: u32,
    terminal_value_sum: f32,
    invalid: bool,
}

struct SearchNode {
    value: f32,
    potential: f32,
    value_samples: u32,
    packed: Option<Vec<u8>>,
    action_count: usize,
    depth: usize,
    visits: u32,
    ranked: Vec<usize>,
    edges: Vec<SearchEdge>,
}

impl SearchNode {
    fn new(
        candidates: Vec<CandidateRow>,
        log_policy: &[f32],
        value: f32,
        potential: f32,
        depth: usize,
        behavior_exponent: f32,
        packed: Option<Vec<u8>>,
    ) -> Self {
        let action_count = candidates.len();
        let behavior_normalizer = log_policy
            .iter()
            .filter(|probability| probability.is_finite())
            .map(|probability| (probability * behavior_exponent).exp())
            .sum::<f32>();
        let mut occurrences = FastMap::<u64, usize>::with_capacity_and_hasher(
            candidates.len(),
            BuildHasherDefault::default(),
        );
        let mut kind_counts = [0; ACTION_KINDS];
        let mut edges = Vec::with_capacity(action_count);
        for (candidate, (row, probability)) in candidates
            .into_iter()
            .zip(log_policy.iter().copied())
            .enumerate()
        {
            let occurrence = occurrences
                .entry(public_candidate_digest(&row))
                .or_default();
            let current_occurrence = *occurrence;
            *occurrence += 1;
            let kind = action_kind(&row.action);
            let kind_occurrence = kind_counts[kind];
            kind_counts[kind] += usize::from(row.legal);
            if row.legal && probability.is_finite() {
                edges.push(SearchEdge {
                    row,
                    candidate,
                    occurrence: current_occurrence,
                    kind_occurrence,
                    prior: probability.exp(),
                    behavior: (probability * behavior_exponent).exp() / behavior_normalizer,
                    rank: 0,
                    visits: 0,
                    value_sum: 0.0,
                    children: Vec::new(),
                    terminal_visits: 0,
                    terminal_value_sum: 0.0,
                    invalid: false,
                });
            }
        }
        let mut order = (0..edges.len()).collect::<Vec<_>>();
        order.sort_by(|&left, &right| edges[right].prior.total_cmp(&edges[left].prior));
        for (rank, &edge) in order.iter().enumerate() {
            edges[edge].rank = rank;
        }
        Self {
            value,
            potential,
            value_samples: 1,
            packed,
            action_count,
            depth,
            visits: 0,
            ranked: order,
            edges,
        }
    }

    fn select(&self, exploration: f32, lane: usize) -> Option<usize> {
        let lane = lane % 16;
        let mut unvisited = [0; 16];
        let mut unvisited_count = 0;
        for &index in &self.ranked {
            if !self.edges[index].invalid && self.edges[index].visits == 0 {
                if unvisited_count < unvisited.len() {
                    unvisited[unvisited_count] = index;
                }
                unvisited_count += 1;
            }
        }
        if unvisited_count > 0 {
            return Some(unvisited[lane % unvisited_count]);
        }
        let visits = self.visits as f32;
        let root = (visits + lane as f32).sqrt();
        let mut best = None;
        for (index, edge) in self.edges.iter().enumerate() {
            if edge.invalid {
                continue;
            }
            let virtual_visits =
                lane / self.edges.len() + usize::from(edge.rank < lane % self.edges.len());
            let score = edge.value_sum / edge.visits as f32
                + exploration * edge.prior * root
                    / (1 + edge.visits as usize + virtual_visits) as f32;
            if best.is_none_or(|(_, best_score): (usize, f32)| {
                best_score.total_cmp(&score) != std::cmp::Ordering::Greater
            }) {
                best = Some((index, score));
            }
        }
        best.map(|(index, _)| index)
    }
}

fn search_candidate<'a>(
    observation: &'a ObservationV56,
    edge: &SearchEdge,
) -> Option<&'a CandidateRow> {
    if let Some(candidate) = observation.candidates.get(edge.candidate)
        && candidate.legal
        && candidate.action == edge.row.action
        && same_public_candidate(candidate, &edge.row)
    {
        return Some(candidate);
    }
    observation
        .candidates
        .iter()
        .filter(|candidate| candidate.legal && same_public_candidate(candidate, &edge.row))
        .nth(edge.occurrence)
        .or_else(|| {
            observation
                .candidates
                .iter()
                .find(|candidate| candidate.legal && same_public_candidate(candidate, &edge.row))
        })
        .or_else(|| {
            if !matches!(edge.row.action, Action::Choose(_)) {
                return None;
            }
            let count = observation
                .candidates
                .iter()
                .filter(|candidate| {
                    candidate.legal && matches!(candidate.action, Action::Choose(_))
                })
                .count();
            (count > 0).then(|| {
                observation
                    .candidates
                    .iter()
                    .filter(|candidate| {
                        candidate.legal && matches!(candidate.action, Action::Choose(_))
                    })
                    .nth(edge.kind_occurrence % count)
                    .unwrap()
            })
        })
        .or_else(|| {
            observation
                .candidates
                .get(edge.candidate)
                .filter(|candidate| candidate.legal)
        })
}

fn search_terminal_value(game: &Game, progress: bool) -> Option<f32> {
    match game.phase {
        Phase::Won => Some(1.0),
        Phase::Dead if progress => {
            Some(canonical_progress(game) as f32 / (TERMINAL_CATEGORIES - 1) as f32)
        }
        Phase::Dead => Some(0.0),
        _ => None,
    }
}

fn heuristic_combat_value(game: &Game) -> f32 {
    let player = game.combat().map_or_else(
        || game.run.hp.max(0) as f32 / game.run.max_hp.max(1) as f32,
        |combat| combat.player.hp.max(0) as f32 / combat.player.max_hp.max(1) as f32,
    );
    let enemy = game.combat().map_or(0.0, |combat| {
        let hp = combat
            .enemies
            .iter()
            .map(|enemy| enemy.creature.hp.max(0) as i32)
            .sum::<i32>();
        let maximum = combat
            .enemies
            .iter()
            .map(|enemy| enemy.creature.max_hp.max(0) as i32)
            .sum::<i32>();
        hp as f32 / maximum.max(1) as f32
    });
    player - 0.1 * enemy
}

struct SearchTree {
    root: Game,
    root_observation: ObservationV56,
    root_digest: u64,
    map: CanonicalMap,
    root_turn: u16,
    simulations: usize,
    budget: usize,
    turns: usize,
    max_depth: usize,
    nodes: Vec<SearchNode>,
    potential_weights: [f32; POTENTIAL_WEIGHT_COUNT],
    lookup: FastMap<(u64, usize), usize>,
    games: Option<Vec<Game>>,
    progress: bool,
    behavior_exponent: f32,
    pack_visits: u32,
    capture_children: bool,
    heuristic: bool,
}

struct SearchLeaf {
    path: SearchPath,
    observation: ObservationV56,
    digest: u64,
    depth: usize,
    game: Option<Game>,
}

struct SearchPath {
    steps: Vec<SearchStep>,
    packed: Vec<(Option<usize>, Vec<u8>)>,
}

struct SearchStep {
    node: usize,
    edge: usize,
    child: Option<usize>,
}

enum SearchResult {
    Value(SearchPath, f32),
    Leaf(SearchLeaf),
    Invalid(SearchPath),
}

impl SearchTree {
    fn new(
        root: &Game,
        observation: &ObservationV56,
        content: &Content,
        layout: Layout,
        log_policy: &[f32],
        value: f32,
        potential_weights: [f32; POTENTIAL_WEIGHT_COUNT],
        budget: usize,
        turns: usize,
        max_depth: usize,
        capture_games: bool,
        progress: bool,
        behavior_exponent: f32,
        pack_visits: u32,
        capture_children: bool,
        heuristic: bool,
    ) -> Self {
        let digest = observation_digest(observation);
        let capacity = budget
            .saturating_mul(if turns == 0 { max_depth.min(16) } else { 1 })
            .saturating_add(1);
        let mut lookup = FastMap::with_capacity_and_hasher(capacity, BuildHasherDefault::default());
        lookup.insert((digest, 0), 0);
        let games = capture_games.then(|| vec![root.clone()]);
        let mut root = root.clone();
        canonicalize_combat_hidden(&mut root);
        let root_turn = root.combat().map_or(0, |combat| combat.turn);
        let map = canonical_map(&root, content, layout);
        let mut nodes = Vec::with_capacity(capacity);
        nodes.push(SearchNode::new(
            observation.candidates.clone(),
            log_policy,
            value,
            potential_value(&observation.potential, &potential_weights),
            0,
            behavior_exponent,
            Some(compact_packed_observation_known(observation, Some(digest))),
        ));
        Self {
            root,
            root_observation: observation.clone(),
            root_digest: digest,
            map,
            root_turn,
            simulations: 0,
            budget,
            turns,
            max_depth,
            nodes,
            potential_weights,
            lookup,
            games,
            progress,
            behavior_exponent,
            pack_visits,
            capture_children,
            heuristic,
        }
    }

    fn horizon(&self, game: &Game) -> bool {
        game.combat().is_none()
            || self.turns > 0
                && game.combat().is_some_and(|combat| {
                    combat.turn >= self.root_turn.saturating_add(self.turns as u16)
                })
    }

    fn simulate(
        &self,
        content: &Content,
        layout: Layout,
        bonuses: (i16, i16),
        seed: u64,
        exploration: f32,
        lane: usize,
    ) -> Result<SearchResult, String> {
        let mut game = self.root.clone();
        resample_canonical_combat_hidden(&mut game, seed);
        let mut observation = None;
        let mut node = 0usize;
        let mut path = SearchPath {
            steps: Vec::with_capacity(8),
            packed: Vec::new(),
        };
        for depth in 1..=self.max_depth {
            let (current, current_digest) = observation
                .as_ref()
                .map(|(observation, digest)| (observation, *digest))
                .unwrap_or((&self.root_observation, self.root_digest));
            let node_packed = self.nodes[node].packed.is_some()
                || path
                    .packed
                    .iter()
                    .any(|(packed_node, _)| *packed_node == Some(node));
            let should_pack = !node_packed
                && self.nodes[node].visits + (lane % 16) as u32 + 1 >= self.pack_visits;
            let pack_children = self.capture_children && (node_packed || should_pack);
            let Some(edge) = self.nodes[node].select(exploration, lane.wrapping_add(depth * 17))
            else {
                return Ok(SearchResult::Invalid(path));
            };
            if should_pack {
                path.packed.push((
                    Some(node),
                    compact_packed_observation_known(current, Some(current_digest)),
                ));
            }
            let search_edge = &self.nodes[node].edges[edge];
            let candidate = search_candidate(current, search_edge).ok_or_else(|| {
                format!(
                    "MCTS public action mismatch: wanted {:?}, have {:?}",
                    search_edge.row.action,
                    current
                        .candidates
                        .iter()
                        .filter(|candidate| candidate.legal)
                        .map(|candidate| &candidate.action)
                        .collect::<Vec<_>>()
                )
            })?;
            let action = candidate.action.clone();
            let was_combat = game.combat().is_some();
            game.step(content, action)
                .map_err(|error| format!("MCTS step failed: {error:?}"))?;
            if !was_combat && game.combat().is_some() {
                apply_training_bonuses(&mut game, content, bonuses.0, bonuses.1);
            }
            if matches!(game.phase, Phase::Won | Phase::Dead) {
                let value = if self.heuristic {
                    heuristic_combat_value(&game)
                } else {
                    search_terminal_value(&game, self.progress).unwrap()
                };
                path.steps.push(SearchStep {
                    node,
                    edge,
                    child: None,
                });
                return Ok(SearchResult::Value(path, value));
            }
            let next_observation =
                observation_v56_with_map(&game, content, layout, bonuses, Some(&self.map));
            let digest = observation_digest(&next_observation);
            let next = self.lookup.get(&(digest, depth)).copied();
            let child_packed = (pack_children
                && next.is_none_or(|index| self.nodes[index].packed.is_none()))
            .then(|| compact_packed_observation_known(&next_observation, Some(digest)));
            if let Some(packed) = child_packed {
                path.packed.push((next, packed));
            }
            if self.horizon(&game) {
                path.steps.push(SearchStep {
                    node,
                    edge,
                    child: next,
                });
                return Ok(if let Some(index) = next {
                    SearchResult::Value(path, self.nodes[index].value)
                } else {
                    SearchResult::Leaf(SearchLeaf {
                        path,
                        observation: next_observation,
                        digest,
                        depth,
                        game: (self.games.is_some() || self.heuristic || self.turns == 0)
                            .then_some(game),
                    })
                });
            }
            if depth >= self.max_depth {
                path.steps.push(SearchStep {
                    node,
                    edge,
                    child: next,
                });
                return Ok(SearchResult::Invalid(path));
            }
            if let Some(next) = next {
                path.steps.push(SearchStep {
                    node,
                    edge,
                    child: Some(next),
                });
                node = next;
                observation = Some((next_observation, digest));
            } else {
                path.steps.push(SearchStep {
                    node,
                    edge,
                    child: None,
                });
                return Ok(SearchResult::Leaf(SearchLeaf {
                    path,
                    observation: next_observation,
                    digest,
                    depth,
                    game: (self.games.is_some() || self.heuristic || self.turns == 0)
                        .then_some(game),
                }));
            }
        }
        unreachable!()
    }

    fn backup(&mut self, path: &mut SearchPath, value: f32) {
        for (node, packed) in path.packed.drain(..) {
            let node = node.expect("unexpanded MCTS packed observation");
            if self.nodes[node].packed.is_none() {
                self.nodes[node].packed = Some(packed);
            }
        }
        for step in &path.steps {
            self.nodes[step.node].visits += 1;
            let edge = &mut self.nodes[step.node].edges[step.edge];
            edge.visits += 1;
            edge.value_sum += value;
            if let Some(child) = step.child {
                if let Some((_, count)) = edge
                    .children
                    .iter_mut()
                    .find(|(candidate, _)| *candidate == child)
                {
                    *count += 1;
                } else {
                    edge.children.push((child, 1));
                }
            } else {
                edge.terminal_visits += 1;
                edge.terminal_value_sum += value;
            }
        }
        self.simulations += 1;
    }

    fn invalidate(&mut self, path: &mut SearchPath) {
        if let Some(step) = path.steps.last() {
            if self.nodes[step.node].packed.is_none()
                && let Some(index) = path
                    .packed
                    .iter()
                    .position(|(node, _)| *node == Some(step.node))
            {
                self.nodes[step.node].packed = Some(path.packed.swap_remove(index).1);
            }
            self.nodes[step.node].edges[step.edge].invalid = true;
        }
        self.simulations += 1;
    }

    fn expand(
        &mut self,
        observation: ObservationV56,
        digest: u64,
        depth: usize,
        mut path: SearchPath,
        mut duplicates: Vec<SearchPath>,
        log_policy: &[f32],
        value: f32,
        game: Option<Game>,
    ) {
        let child = self.insert(observation, digest, depth, log_policy, value, game, None);
        path.steps
            .last_mut()
            .expect("MCTS leaf has an empty path")
            .child = Some(child);
        path.packed
            .iter_mut()
            .filter(|(node, _)| node.is_none())
            .for_each(|(node, _)| *node = Some(child));
        self.backup(&mut path, value);
        for path in &mut duplicates {
            path.steps
                .last_mut()
                .expect("MCTS leaf has an empty path")
                .child = Some(child);
            path.packed
                .iter_mut()
                .filter(|(node, _)| node.is_none())
                .for_each(|(node, _)| *node = Some(child));
            self.backup(path, value);
        }
    }

    fn insert(
        &mut self,
        observation: ObservationV56,
        digest: u64,
        depth: usize,
        log_policy: &[f32],
        value: f32,
        game: Option<Game>,
        packed: Option<Vec<u8>>,
    ) -> usize {
        let key = (digest, depth);
        if let Some(&index) = self.lookup.get(&key) {
            let child = &mut self.nodes[index];
            child.value = (child.value * child.value_samples as f32 + value)
                / (child.value_samples + 1) as f32;
            child.value_samples += 1;
            if child.packed.is_none() {
                child.packed = packed;
            }
            return index;
        }
        let index = self.nodes.len();
        self.lookup.insert(key, index);
        let potential = potential_value(&observation.potential, &self.potential_weights);
        self.nodes.push(SearchNode::new(
            observation.candidates,
            log_policy,
            value,
            potential,
            depth,
            self.behavior_exponent,
            packed,
        ));
        if let Some(games) = &mut self.games {
            games.push(game.expect("captured MCTS node has no game"));
        }
        index
    }

    fn expand_rollout(&mut self, mut leaf: PendingLeaf, rollout: Vec<RolloutStep>, value: f32) {
        for step in rollout {
            let RolloutStep {
                game,
                observation,
                policy,
                digest,
                choice,
                depth,
            } = step;
            let packed = (self.capture_children
                || self.lookup.get(&(digest, depth)).is_some_and(|&index| {
                    let node = &self.nodes[index];
                    node.packed.is_none()
                        && node.visits + 1 >= self.pack_visits
                        && node
                            .edges
                            .iter()
                            .filter(|edge| edge.visits > 0 || edge.candidate == choice)
                            .take(2)
                            .count()
                            >= 2
                }))
            .then(|| compact_packed_observation_known(&observation, Some(digest)));
            let child = self.insert(observation, digest, depth, &policy, value, game, packed);
            let previous = leaf.path.steps.last_mut().expect("empty rollout path");
            previous.child = Some(child);
            leaf.path
                .packed
                .iter_mut()
                .filter(|(node, _)| node.is_none())
                .for_each(|(node, _)| *node = Some(child));
            let edge = self.nodes[child]
                .edges
                .iter()
                .position(|edge| edge.candidate == choice)
                .expect("rollout action is absent from its node");
            leaf.path.steps.push(SearchStep {
                node: child,
                edge,
                child: None,
            });
        }
        self.backup(&mut leaf.path, value);
    }

    fn expectimax(&self) -> (Vec<f32>, Vec<Vec<Option<f32>>>) {
        let mut values = self.nodes.iter().map(|node| node.value).collect::<Vec<_>>();
        let mut actions = self
            .nodes
            .iter()
            .map(|node| vec![None; node.edges.len()])
            .collect::<Vec<_>>();
        let maximum_depth = self.nodes.iter().map(|node| node.depth).max().unwrap_or(0);
        for depth in (0..=maximum_depth).rev() {
            for (index, node) in self
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| node.depth == depth)
            {
                for (edge_index, edge) in node.edges.iter().enumerate() {
                    let visits = edge.visits;
                    if edge.invalid || visits == 0 {
                        continue;
                    }
                    let value = edge.terminal_value_sum
                        + edge
                            .children
                            .iter()
                            .map(|(child, count)| *count as f32 * values[*child])
                            .sum::<f32>();
                    actions[index][edge_index] = Some(value / visits as f32);
                }
                if let Some(value) = actions[index]
                    .iter()
                    .flatten()
                    .copied()
                    .max_by(f32::total_cmp)
                {
                    values[index] = value;
                }
            }
        }
        (values, actions)
    }

    fn target(&self, node: usize, values: &[Option<f32>], temperature: f32) -> Option<Vec<f32>> {
        let mut target = vec![0.0; self.nodes[node].action_count];
        let maximum = values
            .iter()
            .flatten()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        if !maximum.is_finite() || values.iter().flatten().count() < 2 {
            return None;
        }
        let mut sum = 0.0;
        for (edge, value) in self.nodes[node].edges.iter().zip(values) {
            if let Some(value) = value {
                target[edge.candidate] = ((*value - maximum) / temperature).exp();
                sum += target[edge.candidate];
            }
        }
        target.iter_mut().for_each(|value| *value /= sum);
        Some(target)
    }

    fn consistency(&self, node: usize) -> SearchConsistency {
        let mut children = HashMap::<usize, f32>::new();
        let mut covered = 0.0;
        let mut terminal_value = 0.0;
        for edge in &self.nodes[node].edges {
            if edge.invalid || edge.visits == 0 {
                continue;
            }
            let scale = edge.behavior / edge.visits as f32;
            covered += scale * edge.terminal_visits as f32;
            terminal_value += scale * edge.terminal_value_sum;
            for (child, count) in &edge.children {
                if self.nodes[*child].packed.is_some() {
                    covered += scale * *count as f32;
                    *children.entry(*child).or_default() += scale * *count as f32;
                }
            }
        }
        let mut children = children.into_iter().collect::<Vec<_>>();
        children.sort_by_key(|&(child, _)| child);
        let child_potential = children
            .iter()
            .map(|(child, weight)| self.nodes[*child].potential * weight)
            .sum::<f32>();
        let (packed, weights) = children
            .into_iter()
            .map(|(child, weight)| {
                (
                    self.nodes[child]
                        .packed
                        .clone()
                        .expect("consistency child was not packed"),
                    weight,
                )
            })
            .unzip();
        let self_weight = (1.0 - covered).max(0.0);
        SearchConsistency {
            packed,
            weights,
            self_weight,
            terminal_value: terminal_value
                + self_weight * self.nodes[node].potential
                + child_potential
                - self.nodes[node].potential,
        }
    }
}

fn same_public_candidate(left: &CandidateRow, right: &CandidateRow) -> bool {
    left.u == right.u
        && left.s == right.s
        && left.c == right.c
        && left.f == right.f
        && left.legal == right.legal
}

fn public_candidate_digest(row: &CandidateRow) -> u64 {
    let mut digest = 0xcbf2_9ce4_8422_2325u64;
    for value in row
        .u
        .iter()
        .copied()
        .chain(row.s.iter().map(|&value| value as u32))
        .chain(row.c.iter().copied())
        .chain(row.f.iter().map(|value| value.to_bits()))
        .chain(std::iter::once(row.legal as u32))
    {
        digest = (digest ^ value as u64).wrapping_mul(0x100_0000_01b3);
    }
    digest
}

#[derive(Default)]
struct SearchStats {
    turn_starts: usize,
    roots: usize,
    simulations: usize,
    leaves: usize,
    nodes: usize,
    batches: usize,
    targets: usize,
    micros: u64,
    simulate_micros: u64,
    encode_micros: u64,
    inference_micros: u64,
    backup_micros: u64,
    rollout_steps: usize,
    rollout_completed: usize,
    rollout_invalid: usize,
    rollout_micros: u64,
    timed_out: bool,
}

struct ExpertTarget {
    packed: Vec<u8>,
    target: Vec<f32>,
    visits: u32,
    depth: usize,
    consistency: SearchConsistency,
}

#[derive(Default)]
struct SearchConsistency {
    packed: Vec<Vec<u8>>,
    weights: Vec<f32>,
    self_weight: f32,
    terminal_value: f32,
}

struct PendingLeaf {
    tree: usize,
    observation: ObservationV56,
    digest: u64,
    depth: usize,
    path: SearchPath,
    duplicates: Vec<SearchPath>,
    game: Option<Game>,
}

struct RolloutStep {
    game: Option<Game>,
    observation: ObservationV56,
    policy: Vec<f32>,
    digest: u64,
    choice: usize,
    depth: usize,
}

struct CombatRollout {
    value: Option<f32>,
    steps: Vec<RolloutStep>,
}

fn sample_policy(log_policy: &[f32], random: &mut u64) -> Option<usize> {
    let draw = random_f32(random);
    let mut cumulative = 0.0;
    log_policy
        .iter()
        .enumerate()
        .filter(|(_, probability)| probability.is_finite())
        .find_map(|(index, probability)| {
            cumulative += probability.exp();
            (cumulative >= draw).then_some(index)
        })
        .or_else(|| log_policy.iter().rposition(|value| value.is_finite()))
}

fn rollout_combat(
    model: &ValueModel,
    trees: &[SearchTree],
    leaves: &mut [PendingLeaf],
    initial: &[(Vec<f32>, f32, f32, Vec<f32>, Option<Vec<f32>>)],
    content: &Content,
    layout: Layout,
    bonuses: (i16, i16),
    random: &mut u64,
    temperature: f32,
    prior_temperature: f32,
    deadline: Option<std::time::Instant>,
    stats: &mut SearchStats,
) -> Result<Option<Vec<CombatRollout>>, String> {
    let started = std::time::Instant::now();
    let mut games = leaves
        .iter_mut()
        .map(|leaf| {
            if trees[leaf.tree].games.is_some() {
                leaf.game.clone()
            } else {
                leaf.game.take()
            }
            .expect("combat rollout leaf has no game")
        })
        .collect::<Vec<_>>();
    let mut depths = leaves.iter().map(|leaf| leaf.depth).collect::<Vec<_>>();
    let mut values = vec![None; leaves.len()];
    let mut paths = leaves
        .iter()
        .map(|leaf| Vec::with_capacity((trees[leaf.tree].max_depth - leaf.depth).min(16)))
        .collect::<Vec<_>>();
    let mut pending = (0..leaves.len()).collect::<Vec<_>>();
    let mut indices = Vec::with_capacity(leaves.len());
    let mut terminal = Vec::with_capacity(leaves.len());
    let mut actions = vec![None; leaves.len()];
    let mut first = true;
    loop {
        if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            stats.rollout_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            return Ok(None);
        }
        indices.clear();
        terminal.clear();
        for index in pending.drain(..) {
            let game = &games[index];
            let tree = &trees[leaves[index].tree];
            if matches!(game.phase, Phase::Won | Phase::Dead) {
                values[index] = Some(if tree.heuristic {
                    if matches!(game.phase, Phase::Dead) {
                        0.0
                    } else {
                        1.0 + 0.1 * heuristic_combat_value(game)
                    }
                } else {
                    search_terminal_value(game, tree.progress).unwrap()
                });
                stats.rollout_completed += 1;
            } else if game.combat().is_none() {
                if tree.heuristic {
                    values[index] = Some(1.0 + 0.1 * heuristic_combat_value(game));
                    stats.rollout_completed += 1;
                } else {
                    indices.push(index);
                    terminal.push(true);
                }
            } else if depths[index] >= tree.max_depth {
                stats.rollout_invalid += 1;
            } else {
                indices.push(index);
                terminal.push(false);
            }
        }
        if indices.is_empty() {
            break;
        }
        if first {
            for (&index, &terminal) in indices.iter().zip(&terminal) {
                let output = &initial[index];
                if terminal {
                    values[index] = Some(if trees[leaves[index].tree].progress {
                        output.2
                    } else {
                        output.1
                    });
                    stats.rollout_completed += 1;
                } else {
                    let choice = sample_policy(
                        output.4.as_ref().expect("combat rollout policy missing"),
                        random,
                    )
                    .ok_or_else(|| "combat rollout has no legal action".to_owned())?;
                    actions[index] =
                        Some(leaves[index].observation.candidates[choice].action.clone());
                    paths[index].push(RolloutStep {
                        game: trees[leaves[index].tree]
                            .games
                            .is_some()
                            .then(|| games[index].clone()),
                        observation: leaves[index].observation.clone(),
                        policy: output.0.clone(),
                        digest: leaves[index].digest,
                        choice,
                        depth: depths[index],
                    });
                    depths[index] += 1;
                    stats.rollout_steps += 1;
                }
            }
            first = false;
        } else {
            let observations = indices
                .par_iter()
                .map(|&index| {
                    observation_v56_with_map(
                        &games[index],
                        content,
                        layout,
                        bonuses,
                        Some(&trees[leaves[index].tree].map),
                    )
                })
                .collect::<Vec<_>>();
            let rows = observations.iter().collect::<Vec<_>>();
            let encoded = model.state_actions_batch(&rows);
            let outputs = model
                .evaluate_batch(
                    &rows,
                    &encoded,
                    temperature,
                    None,
                    terminal.iter().any(|&terminal| terminal),
                )
                .map_err(|error| error.to_string())?;
            let digests = observations
                .par_iter()
                .zip(&terminal)
                .map(|(observation, &terminal)| {
                    (!terminal).then(|| observation_digest(observation))
                })
                .collect::<Vec<_>>();
            for ((((index, terminal), output), observation), digest) in indices
                .iter()
                .copied()
                .zip(terminal.iter().copied())
                .zip(outputs)
                .zip(observations)
                .zip(digests)
            {
                if terminal {
                    values[index] = Some(if trees[leaves[index].tree].progress {
                        output.2
                    } else {
                        output.1
                    });
                    stats.rollout_completed += 1;
                } else {
                    let choice = sample_policy(&output.0, random)
                        .ok_or_else(|| "combat rollout has no legal action".to_owned())?;
                    let mut policy = output
                        .0
                        .iter()
                        .map(|value| value * temperature / prior_temperature)
                        .collect::<Vec<_>>();
                    let maximum = policy.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                    let normalizer = policy
                        .iter()
                        .filter(|value| value.is_finite())
                        .map(|value| (value - maximum).exp())
                        .sum::<f32>()
                        .ln()
                        + maximum;
                    policy
                        .iter_mut()
                        .filter(|value| value.is_finite())
                        .for_each(|value| *value -= normalizer);
                    let digest = digest.expect("combat rollout digest missing");
                    actions[index] = Some(observation.candidates[choice].action.clone());
                    paths[index].push(RolloutStep {
                        game: trees[leaves[index].tree]
                            .games
                            .is_some()
                            .then(|| games[index].clone()),
                        digest,
                        observation,
                        policy,
                        choice,
                        depth: depths[index],
                    });
                    depths[index] += 1;
                    stats.rollout_steps += 1;
                }
            }
        }
        pending.extend(
            indices
                .iter()
                .copied()
                .zip(&terminal)
                .filter_map(|(index, terminal)| (!terminal).then_some(index)),
        );
        games
            .par_iter_mut()
            .zip(actions.par_iter_mut())
            .try_for_each(|(game, action)| {
                if let Some(action) = action.take() {
                    game.step(content, action)
                        .map_err(|error| format!("combat rollout step failed: {error:?}"))?;
                }
                Ok::<_, String>(())
            })?;
    }
    stats.rollout_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
    Ok(Some(
        values
            .into_iter()
            .zip(paths)
            .map(|(value, steps)| CombatRollout { value, steps })
            .collect(),
    ))
}

fn run_mcts(
    model: &ValueModel,
    trees: &mut [SearchTree],
    content: &Content,
    layout: Layout,
    bonuses: (i16, i16),
    random: &mut u64,
    batch_size: usize,
    prior_temperature: f32,
    policy_temperature: f32,
    exploration: f32,
    deadline: Option<std::time::Instant>,
    stats: &mut SearchStats,
) -> Result<(), String> {
    let mut cursor = 0;
    let mut tasks = Vec::with_capacity(batch_size);
    let mut leaves = Vec::<PendingLeaf>::with_capacity(batch_size);
    let mut leaf_lookup = FastMap::<(usize, u64, usize), usize>::default();
    let mut updates: Vec<Vec<(SearchPath, Option<f32>)>> =
        (0..trees.len()).map(|_| Vec::new()).collect();
    let mut expansions: Vec<Vec<(PendingLeaf, Vec<f32>, f32)>> =
        (0..trees.len()).map(|_| Vec::new()).collect();
    let mut rollout_expansions: Vec<Vec<(PendingLeaf, Vec<RolloutStep>, f32)>> =
        (0..trees.len()).map(|_| Vec::new()).collect();
    while trees.iter().any(|tree| tree.simulations < tree.budget) {
        if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            stats.timed_out = true;
            break;
        }
        tasks.clear();
        for lane in 0..16 {
            for offset in 0..trees.len() {
                let tree = (cursor + offset) % trees.len();
                let search = &trees[tree];
                if search.simulations + lane < search.budget {
                    tasks.push((tree, random_u64(random), lane));
                    if tasks.len() == batch_size {
                        break;
                    }
                }
            }
            if tasks.len() == batch_size {
                break;
            }
        }
        cursor = (cursor + tasks.len()) % trees.len();
        let started = std::time::Instant::now();
        let results = tasks
            .par_iter()
            .map(|&(tree, seed, lane)| {
                trees[tree]
                    .simulate(content, layout, bonuses, seed, exploration, lane)
                    .map(|result| (tree, result))
            })
            .collect::<Result<Vec<_>, _>>()?;
        stats.simulate_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        let started = std::time::Instant::now();
        leaves.clear();
        leaf_lookup.clear();
        updates.iter_mut().for_each(Vec::clear);
        for (tree_index, result) in results {
            match result {
                SearchResult::Value(path, value) => updates[tree_index].push((path, Some(value))),
                SearchResult::Invalid(path) => updates[tree_index].push((path, None)),
                SearchResult::Leaf(leaf) => {
                    let key = (tree_index, leaf.digest, leaf.depth);
                    if trees[tree_index].turns != 0 {
                        if let Some(&index) = leaf_lookup.get(&key) {
                            leaves[index].duplicates.push(leaf.path);
                            continue;
                        }
                        leaf_lookup.insert(key, leaves.len());
                    }
                    leaves.push(PendingLeaf {
                        tree: tree_index,
                        observation: leaf.observation,
                        digest: leaf.digest,
                        depth: leaf.depth,
                        path: leaf.path,
                        duplicates: Vec::new(),
                        game: leaf.game,
                    });
                }
            }
        }
        trees
            .par_iter_mut()
            .zip(&mut updates)
            .for_each(|(tree, updates)| {
                for (mut path, value) in updates.drain(..) {
                    if let Some(value) = value {
                        tree.backup(&mut path, value);
                    } else {
                        tree.invalidate(&mut path);
                    }
                }
            });
        if leaves.is_empty() {
            stats.backup_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            continue;
        }
        stats.backup_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        let rows = leaves
            .iter()
            .map(|leaf| &leaf.observation)
            .collect::<Vec<_>>();
        let started = std::time::Instant::now();
        let features = model.state_actions_batch(&rows);
        stats.encode_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        let started = std::time::Instant::now();
        let rollout = trees.iter().any(|tree| tree.turns == 0);
        let evaluated = model
            .evaluate_batch(
                &rows,
                &features,
                prior_temperature,
                rollout.then_some(policy_temperature),
                !rollout || trees.iter().any(|tree| !tree.heuristic),
            )
            .map_err(|error| error.to_string())?;
        stats.inference_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        let mut rollout_values = if rollout {
            let Some(values) = rollout_combat(
                model,
                trees,
                &mut leaves,
                &evaluated,
                content,
                layout,
                bonuses,
                random,
                policy_temperature,
                prior_temperature,
                deadline,
                stats,
            )?
            else {
                stats.timed_out = true;
                break;
            };
            Some(values)
        } else {
            None
        };
        tracing::debug!(
            tasks = tasks.len(),
            leaves = leaves.len(),
            trees = trees.len(),
            "mcts_wave"
        );
        stats.leaves += leaves.len();
        stats.batches += 1;
        let started = std::time::Instant::now();
        expansions.iter_mut().for_each(Vec::clear);
        rollout_expansions.iter_mut().for_each(Vec::clear);
        for (index, (mut leaf, (policy, win, progress, _, _))) in
            leaves.drain(..).zip(evaluated).enumerate()
        {
            let value = if let Some(rollouts) = &rollout_values {
                rollouts[index].value
            } else if trees[leaf.tree].heuristic {
                Some(heuristic_combat_value(
                    leaf.game.as_ref().expect("heuristic leaf has no game"),
                ))
            } else if trees[leaf.tree].progress {
                Some(progress)
            } else {
                Some(win)
            };
            if let Some(value) = value {
                if let Some(rollouts) = &mut rollout_values {
                    rollout_expansions[leaf.tree].push((
                        leaf,
                        std::mem::take(&mut rollouts[index].steps),
                        value,
                    ));
                } else {
                    expansions[leaf.tree].push((leaf, policy, value));
                }
            } else {
                trees[leaf.tree].invalidate(&mut leaf.path);
            }
        }
        trees
            .par_iter_mut()
            .zip(&mut expansions)
            .zip(&mut rollout_expansions)
            .for_each(|((tree, expansions), rollouts)| {
                for (leaf, rollout, value) in rollouts.drain(..) {
                    tree.expand_rollout(leaf, rollout, value);
                }
                for (leaf, policy, value) in expansions.drain(..) {
                    tree.expand(
                        leaf.observation,
                        leaf.digest,
                        leaf.depth,
                        leaf.path,
                        leaf.duplicates,
                        &policy,
                        value,
                        leaf.game,
                    );
                }
            });
        stats.backup_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
    }
    Ok(())
}

fn mcts_targets(
    model: &ValueModel,
    games: &[Game],
    observations: &[ObservationV56],
    outputs: &[(Vec<f32>, f32, f32, Vec<f32>, Option<Vec<f32>>)],
    turn_starts: &[bool],
    content: &Content,
    layout: Layout,
    bonuses: (i16, i16),
    random: &mut u64,
    fraction: f32,
    simulations: usize,
    boss_simulations: usize,
    turns: usize,
    max_depth: usize,
    batch_size: usize,
    min_visits: u32,
    max_targets: usize,
    prior_temperature: f32,
    q_temperature: f32,
    exploration: f32,
    policy_temperature: f32,
    value_consistency: bool,
    heuristic: bool,
    timeout: f64,
) -> Result<(Vec<ExpertTarget>, SearchStats), String> {
    let turn_start_count = observations
        .iter()
        .zip(turn_starts)
        .filter(|(observation, turn_start)| {
            **turn_start
                && observation
                    .candidates
                    .iter()
                    .filter(|row| row.legal)
                    .count()
                    >= 2
        })
        .count();
    let mut trees = games
        .iter()
        .zip(observations)
        .zip(outputs)
        .zip(turn_starts)
        .filter_map(
            |(((game, observation), (log_policy, _, progress, _, _)), turn_start)| {
                if !turn_start
                    || observation
                        .candidates
                        .iter()
                        .filter(|row| row.legal)
                        .count()
                        < 2
                {
                    return None;
                }
                let forced = matches!(game.room, Room::Elite | Room::Boss);
                let budget = if forced && boss_simulations > 0 {
                    boss_simulations
                } else if simulations > 0 && random_f32(random) < fraction {
                    simulations
                } else {
                    return None;
                };
                let mut prior = log_policy
                    .iter()
                    .map(|value| value * policy_temperature / prior_temperature)
                    .collect::<Vec<_>>();
                let maximum = prior.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                let normalizer = prior
                    .iter()
                    .filter(|value| value.is_finite())
                    .map(|value| (value - maximum).exp())
                    .sum::<f32>()
                    .ln()
                    + maximum;
                prior
                    .iter_mut()
                    .filter(|value| value.is_finite())
                    .for_each(|value| *value -= normalizer);
                Some(SearchTree::new(
                    game,
                    observation,
                    content,
                    layout,
                    &prior,
                    *progress,
                    model.potential_weights,
                    budget,
                    turns,
                    max_depth,
                    false,
                    true,
                    prior_temperature / policy_temperature,
                    min_visits,
                    value_consistency,
                    heuristic,
                ))
            },
        )
        .collect::<Vec<_>>();
    let mut stats = SearchStats {
        turn_starts: turn_start_count,
        roots: trees.len(),
        ..SearchStats::default()
    };
    let deadline = if timeout > 0.0 {
        Some(
            std::time::Instant::now()
                .checked_add(
                    std::time::Duration::try_from_secs_f64(timeout)
                        .map_err(|_| "invalid MCTS timeout")?,
                )
                .ok_or("invalid MCTS timeout")?,
        )
    } else {
        None
    };
    run_mcts(
        model,
        &mut trees,
        content,
        layout,
        bonuses,
        random,
        batch_size,
        prior_temperature,
        policy_temperature,
        exploration,
        deadline,
        &mut stats,
    )?;
    let mut targets = Vec::new();
    for tree in trees {
        stats.simulations += tree.simulations;
        stats.nodes += tree.nodes.len();
        if tree.simulations < tree.budget {
            continue;
        }
        let (_, action_values) = tree.expectimax();
        let mut nodes = tree
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (index, node.visits))
            .filter(|(index, visits)| {
                *visits >= min_visits && action_values[*index].iter().flatten().count() >= 2
            })
            .collect::<Vec<_>>();
        nodes.sort_by_key(|&(index, visits)| (std::cmp::Reverse(visits), index));
        for (index, visits) in nodes.into_iter().take(max_targets) {
            if let Some(target) = tree.target(index, &action_values[index], q_temperature) {
                targets.push(ExpertTarget {
                    packed: tree.nodes[index]
                        .packed
                        .clone()
                        .expect("eligible search target was not packed"),
                    target,
                    visits,
                    depth: tree.nodes[index].depth,
                    consistency: if value_consistency {
                        tree.consistency(index)
                    } else {
                        SearchConsistency {
                            self_weight: 1.0,
                            ..SearchConsistency::default()
                        }
                    },
                });
            }
        }
    }
    stats.targets = targets.len();
    Ok((targets, stats))
}

struct ExactLeaf {
    value: f32,
    player_hp: i16,
    enemy_hp: i32,
    depth: usize,
    weight: usize,
    rng: bool,
    state: serde_json::Value,
}

fn phase_name(phase: &Phase) -> &'static str {
    match phase {
        Phase::Map => "map",
        Phase::Combat(_) => "combat",
        Phase::Rewards(_) => "rewards",
        Phase::Shop(_) => "shop",
        Phase::Rest => "rest",
        Phase::Event(..) => "event",
        Phase::RemoveCards(..) => "remove_cards",
        Phase::UpgradeCards(..) => "upgrade_cards",
        Phase::TransformCards(..) => "transform_cards",
        Phase::EnchantCards(..) => "enchant_cards",
        Phase::ChooseCards(..) => "choose_cards",
        Phase::ChooseBundles(_) => "choose_bundles",
        Phase::Won => "won",
        Phase::Dead => "dead",
    }
}

fn game_state(game: &Game, content: &Content) -> serde_json::Value {
    let card = |card: &Card| {
        serde_json::json!({
            "id": content.cards[card.id as usize].id,
            "instance": card.instance,
            "upgrades": card.upgrades,
            "cost_delta": card.cost_delta,
            "value": card.value,
            "free": card.free,
            "cost_override": card.cost_override,
        })
    };
    let powers = |creature: &Creature| {
        creature
            .powers
            .iter()
            .map(|power| {
                serde_json::json!({
                    "id": content.powers[power.id as usize].id,
                    "amount": power.amount,
                    "value": power.value,
                })
            })
            .collect::<Vec<_>>()
    };
    let creature = |creature: &Creature| {
        serde_json::json!({
            "hp": creature.hp,
            "max_hp": creature.max_hp,
            "block": creature.block,
            "powers": powers(creature),
        })
    };
    let common = serde_json::json!({
        "seed": game.seed,
        "character": content.characters[game.run.character as usize].id,
        "ascension": game.run.ascension,
        "act": game.run.act,
        "floor": game.run.floor,
        "room": format!("{:?}", game.room),
        "phase": phase_name(&game.phase),
        "phase_index": phase_index(&game.phase),
        "actions": game.actions(content).len(),
        "hp": game.run.hp,
        "max_hp": game.run.max_hp,
        "gold": game.run.gold,
        "deck": game.run.deck.iter().map(&card).collect::<Vec<_>>(),
        "relics": game.run.relics.iter().map(|id| content.relics[*id as usize].id).collect::<Vec<_>>(),
        "potions": game.run.potions.iter().map(|id| id.map(|id| content.potions[id as usize].id)).collect::<Vec<_>>(),
    });
    match &game.phase {
        Phase::Combat(combat) => serde_json::json!({
            "common": common,
            "turn": combat.turn,
            "energy": combat.energy,
            "max_energy": combat.max_energy,
            "stars": combat.stars,
            "player": creature(&combat.player),
            "osty": creature(&combat.osty),
            "hand": combat.hand.iter().map(&card).collect::<Vec<_>>(),
            "draw": combat.draw.iter().map(&card).collect::<Vec<_>>(),
            "discard": combat.discard.iter().map(&card).collect::<Vec<_>>(),
            "exhaust": combat.exhaust.iter().map(&card).collect::<Vec<_>>(),
            "orbs": combat.orbs.iter().map(|orb| serde_json::json!({
                "id": content.orbs[orb.id as usize].id,
                "value": orb.value,
            })).collect::<Vec<_>>(),
            "enemies": combat.enemies.iter().map(|enemy| serde_json::json!({
                "instance": enemy.instance,
                "id": content.enemies[enemy.creature.id as usize].id,
                "hp": enemy.creature.hp,
                "max_hp": enemy.creature.max_hp,
                "block": enemy.creature.block,
                "move": enemy.move_index,
                "intent": content.enemies[enemy.creature.id as usize].moves
                    .get(enemy.move_index).map(|movement| movement.intent),
                "powers": powers(&enemy.creature),
            })).collect::<Vec<_>>(),
        }),
        Phase::Rewards(rewards) => serde_json::json!({
            "common": common,
            "reward_gold": rewards.gold,
            "cards": rewards.cards.iter().map(&card).collect::<Vec<_>>(),
            "card_rewards": rewards.card_rewards.iter().map(|reward| format!("{reward:?}")).collect::<Vec<_>>(),
            "relics": rewards.relics.iter().map(|id| content.relics[*id as usize].id).collect::<Vec<_>>(),
            "reward_potions": rewards.potions.iter().map(|id| content.potions[*id as usize].id).collect::<Vec<_>>(),
            "removals": rewards.removals,
        }),
        Phase::Event(id, options) => {
            let event = &content.events[*id as usize];
            serde_json::json!({
                "common": common,
                "event": {
                    "id": event.id,
                    "page": if options.as_slice() == event.options { "initial" } else { "follow_up" },
                    "data": game.event_data,
                    "pending_effects": game.run_queue.iter()
                        .map(|effect| format!("{effect:?}"))
                        .collect::<Vec<_>>(),
                    "resume": game.resume.as_ref().map(phase_name),
                    "options": options.iter().enumerate().map(|(index, option)| serde_json::json!({
                        "index": index,
                        "requirement": format!("{:?}", option.requirement),
                        "effects": option.effects.iter()
                            .map(|effect| format!("{effect:?}"))
                            .collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                },
            })
        }
        _ => common,
    }
}

struct ExactResult {
    value: f32,
    rng: bool,
}

struct ExactSearch<'a> {
    model: &'a ValueModel,
    content: &'a Content,
    layout: Layout,
    bonuses: (i16, i16),
    map: CanonicalMap,
    root_turn: u16,
    turns: usize,
    max_depth: usize,
    max_states: usize,
    states: usize,
    transitions: usize,
    rng_transitions: usize,
    stochastic_splits: usize,
    leaves: Vec<ExactLeaf>,
    action_values: Vec<f32>,
    choices: HashMap<(u64, usize), (Vec<u8>, usize)>,
    record_leaves: bool,
    heuristic: bool,
    cache: EncodingCache,
    progress: bool,
}

impl ExactSearch<'_> {
    fn enter(&mut self) -> Result<(), String> {
        if self.states >= self.max_states {
            return Err(format!(
                "exhaustive search exceeded {} public states",
                self.max_states
            ));
        }
        self.states += 1;
        Ok(())
    }

    fn horizon(&self, game: &Game) -> bool {
        game.combat().is_none()
            || self.turns > 0
                && game.combat().is_some_and(|combat| {
                    combat.turn >= self.root_turn.saturating_add(self.turns as u16)
                })
    }

    fn leaf(
        &mut self,
        particles: &[Game],
        observation: Option<&ObservationV56>,
        depth: usize,
        rng: bool,
    ) -> Result<ExactResult, String> {
        let game = &particles[0];
        let value = if self.heuristic {
            heuristic_combat_value(game)
        } else if let Some(value) = search_terminal_value(game, self.progress) {
            value
        } else {
            let row =
                observation.ok_or_else(|| "missing exhaustive leaf observation".to_owned())?;
            let output = self
                .model
                .evaluate(row, 1.0, &mut self.cache)
                .map_err(|error| error.to_string())?;
            if self.progress { output.2 } else { output.1 }
        };
        self.record_leaf(game, particles.len(), depth, rng, value);
        Ok(ExactResult { value, rng })
    }

    fn record_leaf(&mut self, game: &Game, weight: usize, depth: usize, rng: bool, value: f32) {
        if self.record_leaves {
            let (player_hp, enemy_hp) = game.combat().map_or((game.run.hp, 0), |combat| {
                (
                    combat.player.hp,
                    combat
                        .enemies
                        .iter()
                        .map(|enemy| enemy.creature.hp.max(0) as i32)
                        .sum(),
                )
            });
            self.leaves.push(ExactLeaf {
                value,
                player_hp,
                enemy_hp,
                depth,
                weight,
                rng,
                state: game_state(game, self.content),
            });
        }
    }

    fn flush_leaves(
        &mut self,
        pending: &mut Vec<(f32, usize, Option<Game>, ObservationV56)>,
        depth: usize,
        rng: bool,
    ) -> Result<f32, String> {
        if pending.is_empty() {
            return Ok(0.0);
        }
        if self.heuristic {
            return Ok(pending
                .drain(..)
                .map(|(weight, count, game, _)| {
                    let game = game.expect("heuristic leaf has no game");
                    let value = heuristic_combat_value(&game);
                    self.record_leaf(&game, count, depth, rng, value);
                    weight * value
                })
                .sum());
        }
        if pending.len() == 1 {
            let (weight, count, game, observation) = pending.pop().unwrap();
            let output = self
                .model
                .evaluate(&observation, 1.0, &mut self.cache)
                .map_err(|error| error.to_string())?;
            let value = if self.progress { output.2 } else { output.1 };
            if let Some(game) = game.as_ref() {
                self.record_leaf(game, count, depth, rng, value);
            }
            return Ok(weight * value);
        }
        let evaluated = pending
            .par_iter()
            .map(|(_, _, _, observation)| {
                let index =
                    rayon::current_thread_index().unwrap_or(0) % self.model.encode_caches.len();
                self.model
                    .evaluate(
                        observation,
                        1.0,
                        &mut self.model.encode_caches[index].lock().unwrap(),
                    )
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(pending
            .drain(..)
            .zip(evaluated)
            .map(|((weight, count, game, _), (_, win, progress, _))| {
                let value = if self.progress { progress } else { win };
                if let Some(game) = game.as_ref() {
                    self.record_leaf(game, count, depth, rng, value);
                }
                weight * value
            })
            .sum())
    }

    fn solve(
        &mut self,
        particles: Vec<Game>,
        row: Option<ObservationV56>,
        depth: usize,
        rng: bool,
    ) -> Result<ExactResult, String> {
        self.enter()?;
        if matches!(particles[0].phase, Phase::Won | Phase::Dead) || self.horizon(&particles[0]) {
            return self.leaf(&particles, row.as_ref(), depth, rng);
        }
        if depth >= self.max_depth {
            return Ok(ExactResult {
                value: f32::NEG_INFINITY,
                rng,
            });
        }
        let row = row.ok_or_else(|| "missing exhaustive observation".to_owned())?;
        let policy = row
            .candidates
            .iter()
            .map(|candidate| {
                if candidate.legal {
                    0.0
                } else {
                    f32::NEG_INFINITY
                }
            })
            .collect::<Vec<_>>();
        let node = SearchNode::new(row.candidates.clone(), &policy, 0.0, 0.0, depth, 1.0, None);
        if node.edges.is_empty() {
            return self.leaf(&particles, Some(&row), depth, rng);
        }
        let mut actions = (0..node.edges.len())
            .map(|_| Vec::with_capacity(particles.len()))
            .collect::<Vec<_>>();
        for (particle, game) in particles.iter().enumerate() {
            let observation = (particle > 0).then(|| {
                observation_v56_with_map(
                    game,
                    self.content,
                    self.layout,
                    self.bonuses,
                    Some(&self.map),
                )
            });
            let observation = observation.as_ref().unwrap_or(&row);
            for (actions, edge) in actions.iter_mut().zip(&node.edges) {
                actions.push(
                    search_candidate(observation, edge)
                        .ok_or_else(|| "exhaustive public action mismatch".to_owned())?
                        .action
                        .clone(),
                );
            }
        }
        let mut best = f32::NEG_INFINITY;
        let mut best_choice = 0;
        let mut any_rng = rng;
        let mut groups = HashMap::<u64, (Option<ObservationV56>, Vec<Game>)>::new();
        for (edge, actions) in node.edges.iter().zip(actions) {
            groups.clear();
            let mut action_rng = false;
            let content = self.content;
            let layout = self.layout;
            let bonuses = self.bonuses;
            let map = &self.map;
            let successors = particles
                .par_iter()
                .zip(actions.into_par_iter())
                .map(|(game, action)| {
                    let mut next = game.clone();
                    let action_rng = next
                        .step_with_rng(content, action)
                        .map_err(|error| format!("exhaustive step failed: {error:?}"))?;
                    let (digest, observation) = match next.phase {
                        Phase::Won => (u64::MAX, None),
                        Phase::Dead => (u64::MAX - 1, None),
                        _ => {
                            let observation = observation_v56_with_map(
                                &next,
                                content,
                                layout,
                                bonuses,
                                Some(map),
                            );
                            (observation_digest(&observation), Some(observation))
                        }
                    };
                    Ok((next, action_rng, digest, observation))
                })
                .collect::<Result<Vec<_>, String>>()?;
            for (next, next_rng, digest, observation) in successors {
                action_rng |= next_rng;
                groups
                    .entry(digest)
                    .or_insert_with(|| (observation, Vec::new()))
                    .1
                    .push(next);
            }
            self.transitions += particles.len();
            self.rng_transitions += usize::from(action_rng);
            self.stochastic_splits += usize::from(groups.len() > 1);
            let branch_rng = rng || action_rng || groups.len() > 1;
            let mut value = 0.0;
            let mut pending = Vec::new();
            for (_, (observation, group)) in groups.drain() {
                let weight = group.len() as f32 / particles.len() as f32;
                if self.horizon(&group[0]) && !matches!(group[0].phase, Phase::Won | Phase::Dead) {
                    self.enter()?;
                    let count = group.len();
                    let game = (self.record_leaves || self.heuristic)
                        .then(|| group.into_iter().next().unwrap());
                    pending.push((weight, count, game, observation.unwrap()));
                } else {
                    any_rng |= !pending.is_empty() && branch_rng;
                    value += self.flush_leaves(&mut pending, depth + 1, branch_rng)?;
                    let result = self.solve(group, observation, depth + 1, branch_rng)?;
                    value += weight * result.value;
                    any_rng |= result.rng;
                }
            }
            if !pending.is_empty() {
                any_rng |= branch_rng;
            }
            value += self.flush_leaves(&mut pending, depth + 1, branch_rng)?;
            if depth == 0 {
                self.action_values.push(value);
            }
            if value > best {
                best = value;
                best_choice = edge.candidate;
            }
        }
        if best.is_finite() {
            self.choices.insert(
                (observation_digest(&row), depth),
                (compact_packed_observation(&row), best_choice),
            );
        }
        Ok(ExactResult {
            value: best,
            rng: any_rng,
        })
    }
}

fn exact_search<'a>(
    model: &'a ValueModel,
    game: &Game,
    content: &'a Content,
    layout: Layout,
    bonuses: (i16, i16),
    root_turn: u16,
    turns: usize,
    max_depth: usize,
    samples: usize,
    max_states: usize,
    seed: u64,
    record_leaves: bool,
    progress: bool,
    heuristic: bool,
) -> Result<(ExactResult, ExactSearch<'a>), String> {
    let mut random = seed.max(1);
    let particles = (0..samples)
        .map(|_| {
            let mut particle = game.clone();
            resample_combat_hidden(&mut particle, random_u64(&mut random));
            particle
        })
        .collect::<Vec<_>>();
    let map = canonical_map(&particles[0], content, layout);
    let row = observation_v56_with_map(&particles[0], content, layout, bonuses, Some(&map));
    let mut search = ExactSearch {
        model,
        content,
        layout,
        bonuses,
        map,
        root_turn,
        turns,
        max_depth,
        max_states,
        states: 0,
        transitions: 0,
        rng_transitions: 0,
        stochastic_splits: 0,
        leaves: Vec::new(),
        action_values: Vec::new(),
        choices: HashMap::new(),
        record_leaves,
        heuristic,
        cache: EncodingCache::default(),
        progress,
    };
    let result = search.solve(particles, Some(row), 0, false)?;
    Ok((result, search))
}

fn search_diagnostic(
    model: &ValueModel,
    game: &Game,
    content: &Content,
    layout: Layout,
    bonuses: (i16, i16),
    simulations: usize,
    turns: usize,
    max_depth: usize,
    batch_size: usize,
    prior_temperature: f32,
    exploration: f32,
    policy_temperature: f32,
    exact_samples: usize,
    max_exact_states: usize,
    seed: u64,
    progress: bool,
    q_temperature: f32,
) -> Result<serde_json::Value, String> {
    let observation = observation_v56(game, content, layout, bonuses);
    let (log_policy, win, progress_value, _) = model
        .evaluate(
            &observation,
            policy_temperature,
            &mut EncodingCache::default(),
        )
        .map_err(|error| error.to_string())?;
    let value = if progress { progress_value } else { win };
    let mut prior = log_policy
        .iter()
        .map(|value| value * policy_temperature / prior_temperature)
        .collect::<Vec<_>>();
    let maximum = prior.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let normalizer = prior
        .iter()
        .filter(|value| value.is_finite())
        .map(|value| (value - maximum).exp())
        .sum::<f32>()
        .ln()
        + maximum;
    prior
        .iter_mut()
        .filter(|value| value.is_finite())
        .for_each(|value| *value -= normalizer);
    let mut tree = SearchTree::new(
        game,
        &observation,
        content,
        layout,
        &prior,
        value,
        model.potential_weights,
        simulations,
        turns,
        max_depth,
        true,
        progress,
        1.0,
        u32::MAX,
        false,
        false,
    );
    let mut random = seed.max(1);
    let mut stats = SearchStats {
        roots: 1,
        ..SearchStats::default()
    };
    run_mcts(
        model,
        std::slice::from_mut(&mut tree),
        content,
        layout,
        bonuses,
        &mut random,
        batch_size,
        prior_temperature,
        policy_temperature,
        exploration,
        None,
        &mut stats,
    )?;
    let root_turn = tree.root_turn;
    let (root_exact, exact) = exact_search(
        model,
        game,
        content,
        layout,
        bonuses,
        root_turn,
        turns,
        max_depth,
        exact_samples,
        max_exact_states,
        seed ^ 0x4558_4143_5452_4e47,
        true,
        progress,
        false,
    )?;
    let (search_values, search_actions) = tree.expectimax();
    let games = tree
        .games
        .as_ref()
        .expect("diagnostic MCTS did not capture games");
    let mut nodes = Vec::with_capacity(tree.nodes.len());
    for (index, (node, game)) in tree.nodes.iter().zip(games).enumerate() {
        let visits = node.edges.iter().map(|edge| edge.visits).sum::<u32>();
        let (result, exact_actions, search) = if index == 0 {
            (
                ExactResult {
                    value: root_exact.value,
                    rng: root_exact.rng,
                },
                exact.action_values.clone(),
                None,
            )
        } else {
            let (result, search) = exact_search(
                model,
                game,
                content,
                layout,
                bonuses,
                root_turn,
                turns,
                max_depth.saturating_sub(node.depth).max(1),
                exact_samples,
                max_exact_states,
                seed ^ index as u64,
                false,
                progress,
                false,
            )?;
            let action_values = search.action_values.clone();
            (result, action_values, Some(search))
        };
        let target = tree.target(index, &search_actions[index], q_temperature);
        let actions = node
            .edges
            .iter()
            .zip(&search_actions[index])
            .zip(exact_actions)
            .map(|((edge, approximate), exact)| {
                serde_json::json!({
                    "action": format!("{:?}", edge.row.action),
                    "visits": edge.visits,
                    "prior": edge.prior,
                    "rollout_mean": (edge.visits > 0).then(|| edge.value_sum / edge.visits as f32),
                    "approximate": approximate,
                    "exact": exact,
                    "target": target.as_ref().map(|target| target[edge.candidate]),
                })
            })
            .collect::<Vec<_>>();
        nodes.push(serde_json::json!({
                "node": index,
                "depth": node.depth,
                "visits": visits,
                "model_value": node.value,
                "approximate": search_values[index],
                "exact": result.value,
                "absolute_error": (search_values[index] - result.value).abs(),
                "rng": result.rng,
                "exact_states": search.as_ref().map_or(exact.states, |search| search.states),
                "exact_transitions": search.as_ref().map_or(exact.transitions, |search| search.transitions),
                "rng_transitions": search.as_ref().map_or(exact.rng_transitions, |search| search.rng_transitions),
                "stochastic_splits": search.as_ref().map_or(exact.stochastic_splits, |search| search.stochastic_splits),
                "actions": actions,
            }));
    }
    let root_node = &tree.nodes[0];
    let policy_choice = root_node
        .edges
        .iter()
        .enumerate()
        .max_by(|left, right| {
            log_policy[left.1.candidate].total_cmp(&log_policy[right.1.candidate])
        })
        .map(|(index, _)| index)
        .unwrap_or(0);
    let search_choice = root_node
        .edges
        .iter()
        .enumerate()
        .max_by_key(|(_, edge)| edge.visits)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let search_q_choice = root_node
        .edges
        .iter()
        .enumerate()
        .filter_map(|(index, _)| search_actions[0][index].map(|value| (index, value)))
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(index, _)| index)
        .unwrap_or(search_choice);
    let q_target = tree
        .target(0, &search_actions[0], q_temperature)
        .ok_or_else(|| "root has insufficient Q estimates".to_owned())?;
    let policy_value = root_node
        .edges
        .iter()
        .zip(&exact.action_values)
        .map(|(edge, value)| log_policy[edge.candidate].exp() * value)
        .sum::<f32>();
    let q_target_value = root_node
        .edges
        .iter()
        .zip(&exact.action_values)
        .map(|(edge, value)| q_target[edge.candidate] * value)
        .sum::<f32>();
    let q_target_entropy = -q_target
        .iter()
        .filter(|value| **value > 0.0)
        .map(|value| value * value.ln())
        .sum::<f32>();
    let root_actions = root_node
        .edges
        .iter()
        .enumerate()
        .zip(&exact.action_values)
        .map(|((index, edge), exact)| {
            serde_json::json!({
                "action": format!("{:?}", edge.row.action),
                "policy": log_policy[edge.candidate].exp(),
                "search_prior": edge.prior,
                "visits": edge.visits,
                "rollout_mean": (edge.visits > 0).then(|| edge.value_sum / edge.visits as f32),
                "approximate": search_actions[0][index],
                "exact": exact,
                "q_target": q_target[edge.candidate],
            })
        })
        .collect::<Vec<_>>();
    let leaves = exact
        .leaves
        .iter()
        .map(|leaf| {
            serde_json::json!({
                "value": leaf.value,
                "player_hp": leaf.player_hp,
                "enemy_hp": leaf.enemy_hp,
                "depth": leaf.depth,
                "weight": leaf.weight,
                "rng": leaf.rng,
                "state": leaf.state,
            })
        })
        .collect::<Vec<_>>();
    let combat = game
        .combat()
        .ok_or_else(|| "diagnostic root is not in combat".to_owned())?;
    Ok(serde_json::json!({
        "root": {
            "seed": game.seed,
            "character": game.run.character,
            "act": game.run.act,
            "floor": game.run.floor,
            "turn": combat.turn,
            "player_hp": combat.player.hp,
            "enemy_hp": combat.enemies.iter().map(|enemy| enemy.creature.hp.max(0) as i32).sum::<i32>(),
        },
        "settings": {
            "simulations": simulations,
            "turns": turns,
            "max_depth": max_depth,
            "exact_samples": exact_samples,
            "max_exact_states": max_exact_states,
            "objective": if progress { "progress" } else { "win" },
            "q_temperature": q_temperature,
        },
        "mcts": {
            "simulations": tree.simulations,
            "nodes": tree.nodes.len(),
            "leaves": stats.leaves,
            "batches": stats.batches,
        },
        "exact": {
            "value": root_exact.value,
            "rng": root_exact.rng,
            "states": exact.states,
            "transitions": exact.transitions,
            "rng_transitions": exact.rng_transitions,
            "stochastic_splits": exact.stochastic_splits,
            "leaves": leaves.len(),
        },
        "root_policy": {
            "policy_expected_value": policy_value,
            "policy_greedy_value": exact.action_values.get(policy_choice),
            "search_selected_value": exact.action_values.get(search_choice),
            "search_q_selected_value": exact.action_values.get(search_q_choice),
            "optimal_value": root_exact.value,
            "policy_expected_gap": root_exact.value - policy_value,
            "q_target_expected_value": q_target_value,
            "q_target_gap": root_exact.value - q_target_value,
            "q_target_entropy": q_target_entropy,
            "policy_greedy_gap": exact.action_values.get(policy_choice).map(|value| root_exact.value - value),
            "search_gap": exact.action_values.get(search_choice).map(|value| root_exact.value - value),
            "search_q_gap": exact.action_values.get(search_q_choice).map(|value| root_exact.value - value),
            "actions": root_actions,
        },
        "nodes": nodes,
        "leaves": leaves,
    }))
}

fn random_u64(random: &mut u64) -> u64 {
    *random ^= *random << 13;
    *random ^= *random >> 7;
    *random ^= *random << 17;
    *random
}

fn random_f32(random: &mut u64) -> f32 {
    (random_u64(random) >> 40) as f32 / (1u32 << 24) as f32
}

fn action_descriptor(
    game: &Game,
    content: &Content,
    action: &Action,
) -> (String, Option<String>, Option<String>, Option<String>) {
    let card = |card: Card| Some(content.cards[card.id as usize].id.to_owned());
    let enemy = |index: usize| {
        game.combat()
            .and_then(|combat| combat.enemies.get(index))
            .map(|enemy| format!("creature:{}", enemy.instance.saturating_sub(1)))
    };
    match action {
        Action::Play { hand, target } => {
            let card = game.combat().and_then(|combat| combat.hand.get(*hand));
            (
                "play_card".into(),
                card.map(|card| format!("combat-card:{}", card.instance.saturating_sub(1))),
                target.and_then(enemy),
                card.map(|card| content.cards[card.id as usize].id.to_owned()),
            )
        }
        Action::Potion { slot, target } => (
            "use_potion".into(),
            Some(format!("potion-slot:{slot}")),
            target.and_then(enemy),
            game.run
                .potions
                .get(*slot)
                .and_then(|id| *id)
                .map(|id| content.potions[id as usize].id.to_owned()),
        ),
        Action::DiscardPotion(slot) => (
            "discard_potion".into(),
            Some(format!("potion-slot:{slot}")),
            None,
            game.run
                .potions
                .get(*slot)
                .and_then(|id| *id)
                .map(|id| content.potions[id as usize].id.to_owned()),
        ),
        Action::EndTurn => ("end_turn".into(), None, None, None),
        Action::Path(index) => {
            let node = game.map.nodes.get(*index);
            (
                "map_node".into(),
                None,
                node.map(|node| format!("map:{}:{}", node.lane, node.floor)),
                None,
            )
        }
        Action::RewardCard(index) => {
            let model = match &game.phase {
                Phase::Rewards(rewards) => rewards.cards.get(*index).copied().and_then(card),
                _ => None,
            };
            (
                "reward_card".into(),
                Some(format!("reward-card:{index}")),
                None,
                model,
            )
        }
        Action::RewardRelic(index) => {
            let model = match &game.phase {
                Phase::Rewards(rewards) => rewards
                    .relics
                    .get(*index)
                    .map(|id| content.relics[*id as usize].id.to_owned()),
                _ => None,
            };
            (
                "reward_relic".into(),
                Some(format!("reward-relic:{index}")),
                None,
                model,
            )
        }
        Action::RewardPotion(index) => {
            let model = match &game.phase {
                Phase::Rewards(rewards) => rewards
                    .potions
                    .get(*index)
                    .map(|id| content.potions[*id as usize].id.to_owned()),
                _ => None,
            };
            (
                "reward_potion".into(),
                Some(format!("reward-potion:{index}")),
                None,
                model,
            )
        }
        Action::Buy(index) => {
            let model = match game.phase {
                Phase::Shop(ref items) => items.get(*index).and_then(|item| match item {
                    ShopItem::Card(value, _) => card(*value),
                    ShopItem::Relic(id, _) => Some(content.relics[*id as usize].id.to_owned()),
                    ShopItem::Potion(id, _) => Some(content.potions[*id as usize].id.to_owned()),
                    ShopItem::Remove(_) => Some("card-removal".into()),
                }),
                _ => None,
            };
            ("shop_purchase".into(), model.clone(), None, model)
        }
        Action::Rest => ("rest_site".into(), Some("HEAL".into()), None, None),
        Action::Hatch => ("rest_site".into(), Some("HATCH".into()), None, None),
        Action::Lift => ("rest_site".into(), Some("LIFT".into()), None, None),
        Action::Cook => ("rest_site".into(), Some("COOK".into()), None, None),
        Action::Kindle => ("rest_site".into(), Some("KINDLE".into()), None, None),
        Action::Dig => ("rest_site".into(), Some("DIG".into()), None, None),
        Action::Clone => ("rest_site".into(), Some("CLONE".into()), None, None),
        Action::Smith(index) => (
            "rest_site".into(),
            Some("SMITH".into()),
            None,
            game.run.deck.get(*index).copied().and_then(card),
        ),
        Action::Event(index) | Action::EventRelic(index, _) | Action::EventCard(index, _) => {
            let (event, option) = match &game.phase {
                Phase::Event(id, options) => (
                    Some(content.events[*id as usize].id.to_owned()),
                    options.get(*index),
                ),
                _ => (None, None),
            };
            let detail = match action {
                Action::EventRelic(_, id) => Some(content.relics[*id as usize].id.to_owned()),
                Action::EventCard(_, card) => Some(content.cards[card.id as usize].id.to_owned()),
                Action::Event(index)
                    if matches!(
                        event.as_deref(),
                        Some(
                            "EVENT.NEOW"
                                | "EVENT.DARV"
                                | "EVENT.NONUPEIPE"
                                | "EVENT.OROBAS"
                                | "EVENT.TANX"
                                | "EVENT.TEZCATARA"
                                | "EVENT.PAEL"
                                | "EVENT.VAKUU"
                        )
                    ) && game.event_data[*index] > 0 =>
                {
                    Some(
                        content.relics[game.event_data[*index] as usize - 1]
                            .id
                            .to_owned(),
                    )
                }
                _ => option.map(|option| {
                    let effects = option
                        .effects
                        .iter()
                        .map(|effect| format!("{effect:?}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    if effects.is_empty() {
                        game.event_data
                            .get(*index)
                            .filter(|value| **value != 0)
                            .map_or_else(
                                || "continue".into(),
                                |value| format!("event data {value}"),
                            )
                    } else {
                        effects
                    }
                }),
            };
            (
                "event_option".into(),
                event,
                Some(format!("option:{index}")),
                detail,
            )
        }
        Action::Choose(index) => {
            let chosen = match &game.phase {
                Phase::ChooseCards(cards, ..) => cards.get(*index).copied(),
                _ => game.combat().and_then(|combat| {
                    let choice = combat.choice?;
                    let cards = match choice.pile {
                        Pile::Draw => &combat.draw,
                        Pile::Hand => &combat.hand,
                        Pile::Discard => &combat.discard,
                        Pile::Exhaust => &combat.exhaust,
                        Pile::Offer => &combat.offer,
                    };
                    cards.get(*index).copied()
                }),
            };
            ("choose_cards".into(), None, None, chosen.and_then(card))
        }
        _ => (format!("{action:?}"), None, None, None),
    }
}

fn action_preview(game: &Game, content: &Content, action: &Action) -> Option<String> {
    let value = match action {
        Action::RewardCard(index) => {
            let card = match &game.phase {
                Phase::Rewards(rewards) => *rewards.cards.get(*index)?,
                _ => return None,
            };
            let def = content.cards[card.id as usize];
            serde_json::json!({
                "kind": "card",
                "id": def.id,
                "upgrades": card.upgrades,
                "type": format!("{:?}", def.card_type),
                "rarity": format!("{:?}", def.rarity),
                "cost": def.cost[(card.upgrades > 0) as usize],
                "stars": def.star_cost[(card.upgrades > 0) as usize],
                "target": format!("{:?}", def.target),
                "effects": def.effects.iter().map(|effect| format!("{effect:?}"))
                    .collect::<Vec<_>>(),
            })
        }
        Action::RewardPotion(index) => {
            let id = match &game.phase {
                Phase::Rewards(rewards) => *rewards.potions.get(*index)?,
                _ => return None,
            };
            let def = content.potions[id as usize];
            serde_json::json!({
                "kind": "potion",
                "id": def.id,
                "target": format!("{:?}", def.target),
                "effects": def.effects.iter().map(|effect| format!("{effect:?}"))
                    .collect::<Vec<_>>(),
            })
        }
        _ => return None,
    };
    Some(value.to_string())
}

fn card_score(content: &Content, card: Card) -> f32 {
    let def = content.cards[card.id as usize];
    let rarity = match def.rarity {
        CardRarity::Rare | CardRarity::Ancient => 18.0,
        CardRarity::Uncommon => 8.0,
        CardRarity::Common => 0.0,
        CardRarity::Basic => -12.0,
        CardRarity::Quest | CardRarity::Event => -30.0,
    };
    let kind = match def.card_type {
        CardType::Curse => -60.0,
        CardType::Status => -30.0,
        _ => 0.0,
    };
    let special = match def.id {
        "CARD.FEED"
        | "CARD.GENETIC_ALGORITHM"
        | "CARD.BIG_BANG"
        | "CARD.UNDEATH"
        | "CARD.DEMON_FORM"
        | "CARD.ECHO_FORM"
        | "CARD.BIASED_COGNITION"
        | "CARD.WRAITH_FORM"
        | "CARD.REAPER_FORM" => 120.0,
        "CARD.ANGER"
        | "CARD.INFLAME"
        | "CARD.CORRUPTION"
        | "CARD.FEEL_NO_PAIN"
        | "CARD.BARRICADE"
        | "CARD.BODY_SLAM"
        | "CARD.OFFERING"
        | "CARD.RAMPAGE"
        | "CARD.RAGE"
        | "CARD.INFERNAL_BLADE"
        | "CARD.DEFRAGMENT"
        | "CARD.GLACIER"
        | "CARD.CREATIVE_AI"
        | "CARD.BEAM_CELL"
        | "CARD.REFRACT"
        | "CARD.FERAL"
        | "CARD.BULK_UP"
        | "CARD.MACHINE_LEARNING"
        | "CARD.SHATTER"
        | "CARD.NOXIOUS_FUMES"
        | "CARD.DEADLY_POISON"
        | "CARD.FOOTWORK"
        | "CARD.AFTERIMAGE"
        | "CARD.ADRENALINE"
        | "CARD.BLADE_DANCE"
        | "CARD.ACCURACY"
        | "CARD.BULLET_TIME"
        | "CARD.PHANTOM_BLADES"
        | "CARD.SHADOW_STEP"
        | "CARD.COMET"
        | "CARD.SHINING_STRIKE"
        | "CARD.BULWARK"
        | "CARD.ORBIT"
        | "CARD.SEEKING_EDGE"
        | "CARD.VOID_FORM"
        | "CARD.I_AM_INVINCIBLE"
        | "CARD.THE_SMITH"
        | "CARD.ROYAL_GAMBLE"
        | "CARD.MAKE_IT_SO"
        | "CARD.DECISIONS_DECISIONS"
        | "CARD.COUNTDOWN"
        | "CARD.MISERY"
        | "CARD.NEUROSURGE"
        | "CARD.DEATH_MARCH"
        | "CARD.REANIMATE"
        | "CARD.SACRIFICE"
        | "CARD.ERADICATE"
        | "CARD.PAGESTORM"
        | "CARD.THE_SCYTHE"
        | "CARD.SENTRY_MODE"
        | "CARD.TIMES_UP"
        | "CARD.END_OF_DAYS"
        | "CARD.DREDGE"
        | "CARD.HANG"
        | "CARD.HELIX_DRILL" => 60.0,
        _ => 0.0,
    };
    rarity + kind - def.cost[card.upgrades.min(1) as usize].max(0) as f32 * 2.0
        + card.upgrades as f32 * 6.0
        + card.value as f32 * 20.0
        + special
}

fn engine_score(game: &Game, content: &Content, card: Card) -> f32 {
    let id = content.cards[card.id as usize].id;
    let core = matches!(
        id,
        "CARD.FLAK_CANNON"
            | "CARD.FIEND_FIRE"
            | "CARD.FLAME_BARRIER"
            | "CARD.SECOND_WIND"
            | "CARD.FEEL_NO_PAIN"
            | "CARD.CORRUPTION"
            | "CARD.BODY_SLAM"
            | "CARD.INFERNAL_BLADE"
            | "CARD.HELIX_DRILL"
            | "CARD.BARRAGE"
            | "CARD.GLACIER"
            | "CARD.BEAM_CELL"
            | "CARD.GENETIC_ALGORITHM"
            | "CARD.BUFFER"
            | "CARD.ECHO_FORM"
            | "CARD.RICOCHET"
            | "CARD.BLADE_DANCE"
            | "CARD.FINISHER"
            | "CARD.FLECHETTES"
            | "CARD.AFTERIMAGE"
            | "CARD.WRAITH_FORM"
            | "CARD.REFLECT"
            | "CARD.HEAVENLY_DRILL"
            | "CARD.LUNAR_BLAST"
            | "CARD.RADIATE"
            | "CARD.SEVEN_STARS"
            | "CARD.STARDUST"
            | "CARD.OBLIVION"
            | "CARD.NO_ESCAPE"
            | "CARD.COUNTDOWN"
            | "CARD.NEGATIVE_PULSE"
            | "CARD.END_OF_DAYS"
            | "CARD.NEUROSURGE"
            | "CARD.REAPER_FORM"
            | "CARD.SEVERANCE"
            | "CARD.TIMES_UP"
    );
    let support = matches!(
        id,
        "CARD.WHIRLWIND"
            | "CARD.SWORD_BOOMERANG"
            | "CARD.ANGER"
            | "CARD.RAGE"
            | "CARD.INFLAME"
            | "CARD.OFFERING"
            | "CARD.FEED"
            | "CARD.JUGGERNAUT"
            | "CARD.DARK_EMBRACE"
            | "CARD.BARRICADE"
            | "CARD.BEAM_CELL"
            | "CARD.UPPERCUT"
            | "CARD.CHILL"
            | "CARD.REFRACT"
            | "CARD.COOLHEADED"
            | "CARD.REBOOT"
            | "CARD.DEFRAGMENT"
            | "CARD.FERAL"
            | "CARD.INFINITE_BLADES"
            | "CARD.FOOTWORK"
            | "CARD.PIERCING_WAIL"
            | "CARD.MALAISE"
            | "CARD.FAN_OF_KNIVES"
            | "CARD.BULWARK"
            | "CARD.I_AM_INVINCIBLE"
            | "CARD.ORBIT"
            | "CARD.SHINING_STRIKE"
            | "CARD.HIDDEN_CACHE"
            | "CARD.PAGESTORM"
            | "CARD.CRIMSON_MANTLE"
            | "CARD.SHROUD"
            | "CARD.DEATH_MARCH"
            | "CARD.REAP"
            | "CARD.REAVE"
            | "CARD.SQUEEZE"
    ) && (!matches!(id, "CARD.CRIMSON_MANTLE" | "CARD.SHROUD")
        || game.run.character == 4);
    let secondary = matches!(
        id,
        "CARD.ERADICATE"
            | "CARD.PULL_FROM_BELOW"
            | "CARD.RATTLE"
            | "CARD.RAMPAGE"
            | "CARD.INFERNAL_BLADE"
    );
    let copies = game
        .run
        .deck
        .iter()
        .filter(|owned| owned.id == card.id)
        .count() as f32;
    let repeatable = matches!(
        id,
        "CARD.WHIRLWIND"
            | "CARD.FLAK_CANNON"
            | "CARD.FLAME_BARRIER"
            | "CARD.SWORD_BOOMERANG"
            | "CARD.ANGER"
            | "CARD.RAGE"
            | "CARD.BODY_SLAM"
            | "CARD.FEED"
            | "CARD.BARRAGE"
            | "CARD.BEAM_CELL"
            | "CARD.GLACIER"
            | "CARD.BLADE_DANCE"
            | "CARD.RICOCHET"
            | "CARD.FLECHETTES"
            | "CARD.PIERCING_WAIL"
            | "CARD.HEAVENLY_DRILL"
            | "CARD.LUNAR_BLAST"
            | "CARD.RADIATE"
            | "CARD.SEVEN_STARS"
            | "CARD.STARDUST"
            | "CARD.NO_ESCAPE"
    );
    let bonus = if core {
        220.0
    } else if support {
        100.0
    } else if secondary {
        60.0
    } else {
        0.0
    };
    bonus * (0.55 + game.run.act as f32 * 0.15) - copies * if repeatable { 35.0 } else { 100.0 }
}

fn draft_score(game: &Game, content: &Content, card: Card) -> f32 {
    card_score(content, card) + engine_score(game, content, card)
}

fn upgrade_score(content: &Content, card: Card) -> f32 {
    let id = content.cards[card.id as usize].id;
    let starter = match id {
        "CARD.BASH" | "CARD.ZAP" | "CARD.NEUTRALIZE" | "CARD.VENERATE" | "CARD.UNLEASH" => 2_000.0,
        "CARD.DUALCAST" | "CARD.SURVIVOR" | "CARD.FALLING_STAR" | "CARD.BODYGUARD" => 1_000.0,
        _ => 0.0,
    };
    starter + card_score(content, card)
}

fn engine_card(game: &Game, content: &Content, card: Card) -> bool {
    let copies = game
        .run
        .deck
        .iter()
        .filter(|owned| owned.id == card.id)
        .count() as f32;
    engine_score(game, content, card) + copies * 100.0 > 0.0
}

fn intent_attack(intent: &str) -> (i16, i16) {
    let Some(attack) = intent.split("Attack ").nth(1) else {
        return (0, 0);
    };
    let token = attack.split(',').next().unwrap_or(attack);
    let mut parts = token.split('x');
    let damage = parts
        .next()
        .and_then(|x| x.parse::<i16>().ok())
        .unwrap_or(0);
    let hits = parts
        .next()
        .and_then(|x| x.parse::<i16>().ok())
        .unwrap_or(1);
    (damage, hits)
}

fn potion_score(content: &Content, potion: Id) -> f32 {
    match content.potions[potion as usize].id {
        "POTION.FAIRY_IN_A_BOTTLE" => 10_000.0,
        "POTION.GHOST_IN_A_JAR" | "POTION.ENTROPIC_BREW" => 5_000.0,
        "POTION.FRUIT_JUICE" => 4_000.0,
        _ => 2_000.0,
    }
}

fn rest_distance(game: &Game, node: usize) -> i32 {
    if game.map.nodes[node].room == Room::Rest {
        return 0;
    }
    1 + game.map.nodes[node]
        .next
        .iter()
        .map(|next| rest_distance(game, *next))
        .min()
        .unwrap_or(20)
}

fn elite_count(game: &Game, node: usize) -> i32 {
    (game.map.nodes[node].room == Room::Elite) as i32
        + game.map.nodes[node]
            .next
            .iter()
            .map(|next| elite_count(game, *next))
            .max()
            .unwrap_or(0)
}

fn apply_training_bonuses(game: &mut Game, content: &Content, strength: i16, dexterity: i16) {
    if strength == 0 && dexterity == 0 {
        return;
    }
    let Phase::Combat(combat) = &mut game.phase else {
        return;
    };
    if let Some(id) = content
        .powers
        .iter()
        .position(|power| power.id == "POWER.STRENGTH_POWER")
    {
        combat.player.add_power(id as Id, strength);
    }
    if let Some(id) = content
        .powers
        .iter()
        .position(|power| power.id == "POWER.DEXTERITY_POWER")
    {
        combat.player.add_power(id as Id, dexterity);
    }
}

fn teacher_state_score(game: &Game, content: &Content) -> f32 {
    match game.phase {
        Phase::Won => return 1e9,
        Phase::Dead => return -1e9,
        _ => {}
    }
    let progress = game.run.act.saturating_sub(1) as f32 * 20.0 + game.run.floor as f32;
    let hp = game.combat().map_or(game.run.hp, |combat| combat.player.hp);
    let deck_len = game.run.deck.len() as f32;
    let potion_factor = if game.combat().is_some() && game.room == Room::Boss {
        0.1
    } else {
        1.0
    };
    let mut score = progress * 100_000.0
        + hp as f32 * 500.0
        + game.run.max_hp as f32 * 80.0
        + game.run.gold as f32 * 0.1
        + game.run.relics.len() as f32 * 500.0
        + game
            .run
            .potions
            .iter()
            .flatten()
            .map(|potion| potion_score(content, *potion))
            .sum::<f32>()
            * potion_factor
        + game
            .run
            .deck
            .iter()
            .map(|card| card_score(content, *card) * 10.0)
            .sum::<f32>()
        - deck_len * deck_len * 5.0;
    if matches!(game.phase, Phase::Map) {
        score += game
            .run
            .deck
            .iter()
            .map(|card| engine_score(game, content, *card))
            .sum::<f32>()
            * 5.0;
    }
    if game.run.act == 3 && game.room == Room::Elite {
        let reserve = game.run.max_hp * 2 / 3;
        score -= (reserve - hp).max(0) as f32 * 750.0;
    }
    if game.run.character == 0 {
        for (index, card) in game.run.deck.iter().enumerate() {
            let id = content.cards[card.id as usize].id;
            let repeatable = matches!(
                id,
                "CARD.WHIRLWIND"
                    | "CARD.FLAK_CANNON"
                    | "CARD.FLAME_BARRIER"
                    | "CARD.SWORD_BOOMERANG"
                    | "CARD.ANGER"
                    | "CARD.RAGE"
                    | "CARD.BARRAGE"
                    | "CARD.BLADE_DANCE"
                    | "CARD.RICOCHET"
                    | "CARD.FLECHETTES"
                    | "CARD.HEAVENLY_DRILL"
                    | "CARD.LUNAR_BLAST"
                    | "CARD.RADIATE"
                    | "CARD.SEVEN_STARS"
                    | "CARD.STARDUST"
                    | "CARD.NO_ESCAPE"
            );
            if !repeatable
                && game.run.deck[..index]
                    .iter()
                    .any(|prior| prior.id == card.id)
            {
                score -= 700.0;
            }
        }
    }
    if game.run.character == 0 {
        let count = |id| {
            game.run
                .deck
                .iter()
                .filter(|card| content.cards[card.id as usize].id == id)
                .count() as f32
        };
        let rage = count("CARD.RAGE");
        let anger = count("CARD.ANGER");
        let available = |id| {
            game.combat().is_none_or(|combat| {
                combat
                    .draw
                    .iter()
                    .chain(&combat.hand)
                    .chain(&combat.discard)
                    .any(|card| content.cards[card.id as usize].id == id)
                    || id == "CARD.FEEL_NO_PAIN"
                        && combat.player.powers.iter().any(|power| {
                            content.powers[power.id as usize].id == "POWER.FEEL_NO_PAIN_POWER"
                        })
            })
        };
        score += 500.0 * rage.min(3.0)
            + 500.0 * anger.min(2.0)
            + 700.0
                * [
                    "CARD.INFLAME",
                    "CARD.OFFERING",
                    "CARD.FEED",
                    "CARD.JUGGERNAUT",
                ]
                .iter()
                .map(|id| count(id).min(1.0))
                .sum::<f32>()
            + 300.0
                * ["CARD.RAMPAGE", "CARD.INFERNAL_BLADE"]
                    .iter()
                    .map(|id| count(id).min(1.0))
                    .sum::<f32>()
            + 1_000.0 * (rage > 0.0 && anger > 0.0) as u8 as f32
            + 1_000.0 * (rage > 0.0 && count("CARD.INFLAME") > 0.0) as u8 as f32
            + 1_000.0 * (rage > 0.0 && count("CARD.JUGGERNAUT") > 0.0) as u8 as f32
            + 1_000.0
                * (count("CARD.FEEL_NO_PAIN") > 0.0
                    && count("CARD.SECOND_WIND") > 0.0
                    && count("CARD.BODY_SLAM") > 0.0
                    && available("CARD.FEEL_NO_PAIN")
                    && available("CARD.SECOND_WIND")
                    && available("CARD.BODY_SLAM")) as u8 as f32;
    }
    if game.run.character == 4 {
        let count = |id| {
            game.run
                .deck
                .iter()
                .filter(|card| content.cards[card.id as usize].id == id)
                .count() as f32
        };
        let no_escape = count("CARD.NO_ESCAPE");
        let countdown = count("CARD.COUNTDOWN");
        score += 1_000.0 * no_escape.min(3.0)
            + 700.0 * countdown.min(2.0)
            + 1_000.0
                * ["CARD.END_OF_DAYS", "CARD.PAGESTORM", "CARD.MAD_SCIENCE"]
                    .iter()
                    .map(|id| count(id).min(1.0))
                    .sum::<f32>()
            + 3_000.0 * (no_escape >= 2.0 && countdown > 0.0) as u8 as f32
            + 2_000.0 * count("CARD.REAPER_FORM").min(1.0)
            + 800.0
                * [
                    "CARD.DEATH_MARCH",
                    "CARD.TIMES_UP",
                    "CARD.REAP",
                    "CARD.REAVE",
                ]
                .iter()
                .map(|id| count(id).min(1.0))
                .sum::<f32>()
            + 2_500.0 * (count("CARD.REAPER_FORM") > 0.0 && countdown > 0.0) as u8 as f32;
    }
    if let Some(combat) = game.combat() {
        let test_subject = combat
            .enemies
            .iter()
            .any(|enemy| content.enemies[enemy.creature.id as usize].id == "MONSTER.TEST_SUBJECT");
        let aeonglass = combat
            .enemies
            .iter()
            .any(|enemy| content.enemies[enemy.creature.id as usize].id == "MONSTER.AEONGLASS");
        let nemesis = combat.enemies.iter().any(|enemy| {
            enemy
                .creature
                .powers
                .iter()
                .any(|power| content.powers[power.id as usize].id == "POWER.NEMESIS_POWER")
        });
        let (incoming, attacks) = combat
            .enemies
            .iter()
            .filter(|enemy| enemy.creature.hp > 0)
            .fold((0i16, 0i16), |(incoming, attacks), enemy| {
                let def = &content.enemies[enemy.creature.id as usize];
                let (damage, mut hits) = intent_attack(def.moves[enemy.move_index].intent);
                if def.id == "MONSTER.TEST_SUBJECT" && enemy.move_index == 3 {
                    hits = 2 + enemy
                        .move_history
                        .iter()
                        .filter(|&&prior| prior == 3)
                        .count() as i16;
                }
                (
                    incoming.saturating_add(damage.saturating_mul(hits)),
                    attacks.saturating_add(hits),
                )
            });
        let reflect = combat.player.powers.iter().any(|power| {
            power.amount > 0 && content.powers[power.id as usize].id == "POWER.REFLECT_POWER"
        });
        let barricade = combat.player.powers.iter().any(|power| {
            power.amount > 0 && content.powers[power.id as usize].id == "POWER.BARRICADE_POWER"
        });
        let useful_block = if barricade {
            combat.player.block
        } else if test_subject {
            combat.player.block.min(incoming)
        } else {
            combat.player.block.min(incoming.max(20))
        };
        score += useful_block as f32
            * if nemesis && reflect && attacks > 0 {
                185.0
            } else {
                35.0
            };
        score += combat.energy as f32 * 2.0
            + combat.stars as f32 * 100.0
            + combat.osty.hp.max(0) as f32 * 50.0;
        score += combat
            .draw
            .iter()
            .chain(&combat.hand)
            .chain(&combat.discard)
            .map(|card| {
                card.value as f32 * 20.0
                    - match card.id {
                        card_id::WOUND => 750.0,
                        card_id::BURN => 1_500.0,
                        card_id::WITHER => 750.0 + 770.0 * card.value.max(0) as f32,
                        _ => 0.0,
                    }
            })
            .sum::<f32>();
        score += combat
            .exhaust
            .iter()
            .map(|card| card.value as f32 * 20.0)
            .sum::<f32>();
        for orb in &combat.orbs {
            let def = content.orbs[orb.id as usize];
            score += match def.id {
                "ORB.LIGHTNING_ORB" => 150.0,
                "ORB.FROST_ORB" => 300.0,
                "ORB.DARK_ORB" | "ORB.GLASS_ORB" => orb.value as f32 * 45.0,
                "ORB.PLASMA_ORB" => 200.0,
                _ => 0.0,
            };
        }
        for power in &combat.player.powers {
            let def = content.powers[power.id as usize];
            let weight = match def.id {
                "POWER.STRENGTH_POWER" => 250.0,
                "POWER.DEXTERITY_POWER" => 200.0,
                "POWER.FOCUS_POWER" => 300.0,
                "POWER.INTANGIBLE_POWER" => 1_000.0,
                "POWER.COUNTDOWN_POWER" if game.room == Room::Boss => match power.amount {
                    ..=9 => 800.0,
                    10..=15 => 500.0,
                    _ => 350.0,
                },
                "POWER.REAPER_FORM_POWER"
                    if aeonglass || nemesis || game.room == Room::Boss && !test_subject =>
                {
                    20_000.0
                }
                "POWER.ECHO_FORM_POWER" => 2_000.0,
                "POWER.CREATIVE_AI_POWER" => 1_200.0,
                "POWER.MACHINE_LEARNING_POWER" => 600.0,
                "POWER.LOOP_POWER" => 500.0,
                "POWER.FEEL_NO_PAIN_POWER" => 400.0,
                "POWER.RAGE_POWER" => 300.0,
                "POWER.FLAME_BARRIER_POWER" => attacks as f32 * 150.0,
                _ if nemesis && matches!(def.kind, PowerKind::Thorns) => attacks as f32 * 150.0,
                _ if def.debuff => -80.0,
                _ => 80.0,
            };
            score += power.amount as f32 * weight;
        }
        for enemy in &combat.enemies {
            let enemy_id = content.enemies[enemy.creature.id as usize].id;
            let adaptable = enemy
                .creature
                .powers
                .iter()
                .any(|power| content.powers[power.id as usize].id == "POWER.ADAPTABLE_POWER");
            let future_hp = if adaptable && enemy_id == "MONSTER.TEST_SUBJECT" {
                if enemy.creature.max_hp < 200 {
                    525
                } else {
                    313
                }
            } else {
                0
            };
            score -= (enemy.creature.hp.max(0) + future_hp) as f32
                * if matches!(
                    enemy_id,
                    "MONSTER.TORCH_HEAD_AMALGAM"
                        | "MONSTER.TEST_SUBJECT"
                        | "MONSTER.AEONGLASS"
                        | "MONSTER.QUEEN"
                ) {
                    1_000.0
                } else {
                    150.0
                };
            score -= enemy.creature.block as f32 * 5.0;
            for power in &enemy.creature.powers {
                let def = content.powers[power.id as usize];
                score += power.amount as f32
                    * if def.id == "POWER.SANDPIT_POWER" {
                        2_000.0
                    } else if def.id == "POWER.POISON_POWER" {
                        150.0
                    } else if def.debuff {
                        80.0
                    } else {
                        -80.0
                    };
            }
        }
        score -= (incoming - combat.player.block).max(0) as f32 * 250.0;
    } else {
        score += 20_000.0;
    }
    score
}

fn combat_quality(game: &Game) -> f32 {
    if matches!(game.phase, Phase::Dead) {
        return 0.0;
    }
    let hp = game
        .combat()
        .map_or(game.run.hp, |combat| combat.player.hp)
        .max(0) as f32
        / game.run.max_hp.max(1) as f32;
    let Some(combat) = game.combat() else {
        return 1.0 + 0.25 * hp;
    };
    let future = |enemy: &Enemy| {
        if enemy
            .creature
            .powers
            .iter()
            .any(|power| power.id == power_id::ADAPTABLE)
        {
            if enemy.creature.max_hp < 200 {
                525
            } else {
                313
            }
        } else {
            0
        }
    };
    let current: i32 = combat
        .enemies
        .iter()
        .map(|enemy| enemy.creature.hp.max(0) as i32 + future(enemy))
        .sum();
    let maximum: i32 = combat
        .enemies
        .iter()
        .map(|enemy| enemy.creature.max_hp.max(1) as i32 + future(enemy))
        .sum();
    0.5 * hp + 0.5 * (1.0 - current as f32 / maximum.max(1) as f32)
}

fn teacher_choice(game: &Game, content: &Content) -> usize {
    let actions = game.actions(content);
    if let Phase::Event(id, _) = game.phase
        && content.events[id as usize].id == "EVENT.REFLECTIONS"
        && let Some(index) = actions
            .iter()
            .position(|action| matches!(action, Action::Event(0)))
    {
        return index;
    }
    if game
        .crystal
        .as_ref()
        .is_some_and(|crystal| crystal.remaining > 0)
    {
        return actions
            .iter()
            .position(|action| matches!(action, Action::CrystalCell(..)))
            .unwrap_or(0);
    }
    if actions
        .iter()
        .all(|action| matches!(action, Action::DiscardPotion(_)))
    {
        return actions
            .iter()
            .enumerate()
            .min_by(|a, b| {
                let score = |action: &Action| match action {
                    Action::DiscardPotion(slot) => game.run.potions[*slot]
                        .map(|potion| potion_score(content, potion))
                        .unwrap_or_default(),
                    _ => 0.0,
                };
                score(a.1).total_cmp(&score(b.1))
            })
            .map_or(0, |x| x.0);
    }
    if matches!(game.phase, Phase::Map) {
        let hurt = game.run.hp * 2 < game.run.max_hp;
        return actions
            .iter()
            .enumerate()
            .filter(|(_, action)| matches!(action, Action::Path(_)))
            .max_by_key(|(_, action)| match action {
                Action::Path(path) if hurt => {
                    let room = match game.map.nodes[*path].room {
                        Room::Rest => 200,
                        Room::Shop if game.run.gold >= 100 => 80,
                        Room::Treasure => 50,
                        Room::Unknown => 20,
                        Room::Combat => -50,
                        Room::Elite => -80,
                        Room::Boss => -100,
                        _ => 0,
                    };
                    room - rest_distance(game, *path) * 20
                }
                Action::Path(path) => {
                    let room = match game.map.nodes[*path].room {
                        Room::Elite if game.run.act == 1 => 120,
                        Room::Shop if game.run.gold >= 100 => 99,
                        Room::Treasure => 90,
                        Room::Unknown => 80,
                        Room::Rest => 60,
                        Room::Combat => 50,
                        Room::Shop => 40,
                        Room::Elite => 10,
                        Room::Boss => 0,
                        _ => 20,
                    };
                    room + if game.run.act == 1 {
                        elite_count(game, *path) * 50
                    } else {
                        0
                    }
                }
                _ => 0,
            })
            .map_or(0, |x| x.0);
    }
    if let Phase::Shop(items) = &game.phase
            && (game.run.deck.len() > 18
                || game.run.deck.iter().any(|card| {
                    let def = content.cards[card.id as usize];
                    matches!(def.card_type, CardType::Curse | CardType::Status)
                        && card.flags(def) & ETERNAL == 0
                }))
            && let Some((index, _)) = actions.iter().enumerate().find(|(_, action)| {
                matches!(action, Action::Buy(item) if matches!(items[*item], ShopItem::Remove(_)))
            })
        {
            return index;
        }
    if let Phase::Shop(items) = &game.phase
        && let Some((index, score)) = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                Action::Buy(item) => Some((
                    index,
                    match items[*item] {
                        ShopItem::Card(card, _) => draft_score(game, content, card),
                        ShopItem::Relic(..) => 180.0,
                        _ => 0.0,
                    },
                )),
                _ => None,
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
        && score > 50.0
    {
        return index;
    }
    if matches!(game.phase, Phase::Shop(_))
        && let Some(index) = actions
            .iter()
            .position(|action| matches!(action, Action::Cancel | Action::Leave))
    {
        return index;
    }
    if let Phase::Rewards(rewards) = &game.phase {
        for kind in [8, 6, 9] {
            if let Some((index, _)) = actions
                .iter()
                .enumerate()
                .find(|(_, action)| action_kind(action) == kind)
            {
                return index;
            }
        }
        if !rewards.cards.is_empty()
            && let Some((index, _)) = actions
                .iter()
                .enumerate()
                .filter_map(|(index, action)| {
                    Some((
                        index,
                        match action {
                            Action::RewardCard(card) => {
                                let card = rewards.cards[*card];
                                draft_score(game, content, card)
                                    - game
                                        .run
                                        .deck
                                        .iter()
                                        .filter(|owned| owned.id == card.id)
                                        .count() as f32
                                        * 30.0
                                    - game.run.deck.len().saturating_sub(12) as f32 * 2.0
                            }
                            Action::Cancel => 40.0,
                            _ => return None,
                        },
                    ))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
        {
            return index;
        }
        for kind in [10, 29, 31] {
            if let Some((index, _)) = actions
                .iter()
                .enumerate()
                .find(|(_, action)| action_kind(action) == kind)
            {
                return index;
            }
        }
    }
    if matches!(
        game.phase,
        Phase::RemoveCards(..) | Phase::TransformCards(..)
    ) {
        return actions
            .iter()
            .enumerate()
            .min_by(|a, b| {
                let score = |action: &Action| match action {
                    Action::RemoveCard(card) => draft_score(game, content, game.run.deck[*card]),
                    Action::Cancel => 10_000.0,
                    _ => 9_999.0,
                };
                score(a.1).total_cmp(&score(b.1))
            })
            .map_or(0, |x| x.0);
    }
    if matches!(game.phase, Phase::UpgradeCards(..)) {
        return actions
            .iter()
            .enumerate()
            .max_by(|a, b| {
                let score = |action: &Action| match action {
                    Action::Smith(card) => {
                        upgrade_score(content, game.run.deck[*card])
                            + engine_score(game, content, game.run.deck[*card])
                    }
                    _ => -10_000.0,
                };
                score(a.1).total_cmp(&score(b.1))
            })
            .map_or(0, |x| x.0);
    }
    if matches!(game.phase, Phase::Rest) {
        if game.run.hp * 4 < game.run.max_hp * 3
            && let Some(index) = actions
                .iter()
                .position(|action| matches!(action, Action::Rest))
        {
            return index;
        }
        for special in [
            Action::Cook,
            Action::Hatch,
            Action::Lift,
            Action::Dig,
            Action::Kindle,
        ] {
            if let Some(index) = actions.iter().position(|action| *action == special) {
                return index;
            }
        }
        if let Some((index, _)) = actions
            .iter()
            .enumerate()
            .filter(|(_, action)| matches!(action, Action::Smith(_)))
            .max_by(|a, b| {
                let score = |action: &Action| match action {
                    Action::Smith(card) => {
                        upgrade_score(content, game.run.deck[*card])
                            + engine_score(game, content, game.run.deck[*card])
                    }
                    _ => -10_000.0,
                };
                score(a.1).total_cmp(&score(b.1))
            })
        {
            return index;
        }
    }
    actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            if matches!(action, Action::DiscardPotion(_)) {
                return Some((index, -2e9));
            }
            let mut next = game.clone();
            next.step(content, action.clone()).ok()?;
            Some((index, teacher_state_score(&next, content)))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(0, |x| x.0)
}

fn rescue_choice(
    game: &Game,
    content: &Content,
    choice: usize,
    danger: f32,
    tactical: bool,
) -> usize {
    let actions = game.actions(content);
    let rest_threshold = match game.run.character {
        0 | 2 => 3,
        _ => 4,
    };
    if matches!(game.phase, Phase::Rest)
        && game.run.hp * 10 <= game.run.max_hp * rest_threshold
        && let Some(rest) = actions
            .iter()
            .position(|action| matches!(action, Action::Rest))
    {
        return rest;
    }
    if tactical
        && matches!(game.run.character, 1 | 3 | 4)
        && matches!(
            game.phase,
            Phase::Map
                | Phase::Shop(_)
                | Phase::Rewards(_)
                | Phase::RemoveCards(..)
                | Phase::TransformCards(..)
                | Phase::UpgradeCards(..)
                | Phase::Rest
        )
    {
        return teacher_choice(game, content);
    }
    if game.run.character == 2
        && game.run.hp * 5 < game.run.max_hp * 2
        && matches!(game.phase, Phase::Map)
        && actions.get(choice).is_some_and(|action| {
            matches!(
                action,
                Action::Path(path)
                    if matches!(game.map.nodes[*path].room, Room::Combat | Room::Elite)
            )
        })
    {
        let safer = teacher_choice(game, content);
        if actions.get(safer).is_some_and(|action| {
                matches!(
                    action,
                    Action::Path(path)
                        if !matches!(game.map.nodes[*path].room, Room::Combat | Room::Elite | Room::Boss)
                )
            }) {
                return safer;
            }
    }
    if matches!(actions.get(choice), Some(Action::Play { .. }))
        && game
            .combat()
            .is_some_and(|combat| combat.history.manual_plays >= 32)
        && let Some(end_turn) = actions
            .iter()
            .position(|action| matches!(action, Action::EndTurn))
    {
        return end_turn;
    }
    if !matches!(actions.get(choice), Some(Action::EndTurn)) {
        return choice;
    }
    let Some(combat) = game.combat() else {
        return choice;
    };
    let incoming = combat
        .enemies
        .iter()
        .filter(|enemy| enemy.creature.hp > 0)
        .map(|enemy| {
            let def = &content.enemies[enemy.creature.id as usize];
            let (mut damage, mut hits) = intent_attack(def.moves[enemy.move_index].intent);
            if def.id == "MONSTER.TEST_SUBJECT" && enemy.move_index == 3 {
                damage = if game.run.ascension >= 9 { 11 } else { 10 };
                hits = 2 + enemy
                    .move_history
                    .iter()
                    .filter(|&&prior| prior == 3)
                    .count() as i16;
            } else if def.id == "MONSTER.WATERFALL_GIANT" && enemy.move_index == 4 {
                damage = (if game.run.ascension >= 9 { 23 } else { 20 }) + enemy.value;
                hits = 1;
            } else if def.id == "MONSTER.WATERFALL_GIANT" && enemy.move_index == 6 {
                damage = enemy.value;
                hits = 1;
            }
            damage.saturating_mul(hits)
        })
        .fold(0i16, i16::saturating_add);
    let damage = incoming.saturating_sub(combat.player.block).max(0);
    if tactical
        && combat.history.manual_cards < 6
        && (damage as f32) >= combat.player.hp.max(1) as f32 * danger
    {
        return actions
            .iter()
            .enumerate()
            .filter(|(_, action)| matches!(action, Action::Play { .. } | Action::Potion { .. }))
            .filter_map(|(index, action)| {
                let consumed = match action {
                    Action::Potion { slot, .. } => game.run.potions[*slot]
                        .map(|id| potion_score(content, id))
                        .unwrap_or_default(),
                    _ => 0.0,
                };
                let mut next = game.clone();
                resample_hidden(&mut next, content, 0x5055_424c_4943_0001);
                next.step(content, action.clone()).ok()?;
                Some((index, teacher_state_score(&next, content) + consumed))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(choice, |(index, _)| index);
    }
    if (damage as f32) < combat.player.hp.max(1) as f32 * danger {
        return choice;
    }
    actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            let Action::Potion { slot, target } = action else {
                return None;
            };
            let id = content.potions[game.run.potions[*slot]? as usize].id;
            let priority = match id {
                "POTION.GHOST_IN_A_JAR" => 100,
                "POTION.BLOCK_POTION" | "POTION.SHIP_IN_A_BOTTLE" => 90,
                "POTION.SHACKLING_POTION" | "POTION.WEAK_POTION" | "POTION.POTION_OF_BINDING" => 80,
                "POTION.FIRE_POTION"
                | "POTION.EXPLOSIVE_AMPOULE"
                | "POTION.POTION_SHAPED_ROCK"
                | "POTION.POTION_OF_DOOM"
                | "POTION.POWDERED_DEMISE" => 70,
                "POTION.FAIRY_IN_A_BOTTLE" => 0,
                "POTION.FOUL_POTION" if combat.player.hp <= 12 => 0,
                _ => 60,
            };
            let target_hp = target
                .and_then(|target| combat.enemies.get(target))
                .map_or(0, |enemy| enemy.creature.hp.max(0));
            Some((index, priority, -target_hp))
        })
        .filter(|(_, priority, _)| *priority > 0)
        .max_by_key(|(_, priority, target)| (*priority, *target))
        .map_or(choice, |(index, _, _)| index)
}

fn teacher_plan(game: &Game, content: &Content, width: usize, teacher_turns: usize) -> Vec<Action> {
    let actions = game.actions(content);
    let Some(turn) = game.combat().map(|combat| combat.turn) else {
        return actions
            .get(teacher_choice(game, content))
            .cloned()
            .into_iter()
            .collect();
    };
    if !actions
        .iter()
        .any(|action| matches!(action, Action::Play { .. } | Action::EndTurn))
    {
        return actions.into_iter().take(1).collect();
    }
    if width == 1 {
        return actions
            .get(teacher_choice(game, content))
            .cloned()
            .into_iter()
            .collect();
    }
    let turns = teacher_turns.max((game.room == Room::Boss) as usize + 1) as u16;
    let mut frontier = vec![(game.clone(), Vec::new())];
    let mut best: Option<(f32, Vec<Action>)> = None;
    let mut fallback: Option<(f32, Vec<Action>)> = None;
    let mut expansions = 0;
    let expansion_limit = 256;
    'search: for _ in 0..10 * turns as usize {
        let mut next_frontier = Vec::new();
        for (state, plan) in frontier {
            let actions = state.actions(content);
            let action_limit = if actions
                .iter()
                .any(|action| matches!(action, Action::Play { .. } | Action::EndTurn))
            {
                64
            } else {
                1
            };
            for action in actions
                .into_iter()
                .filter(|action| !matches!(action, Action::DiscardPotion(_)))
                .take(action_limit)
            {
                if expansions == expansion_limit {
                    break 'search;
                }
                expansions += 1;
                let mut next = state.clone();
                if next.step(content, action.clone()).is_err() {
                    continue;
                }
                let mut next_plan = plan.clone();
                next_plan.push(action);
                let candidate = (teacher_state_score(&next, content), next_plan.clone());
                if fallback
                    .as_ref()
                    .is_none_or(|fallback| candidate.0 > fallback.0)
                {
                    fallback = Some(candidate.clone());
                }
                if next
                    .combat()
                    .is_none_or(|combat| combat.turn >= turn.saturating_add(turns))
                {
                    if best.as_ref().is_none_or(|best| candidate.0 > best.0) {
                        best = Some(candidate);
                    }
                } else {
                    next_frontier.push((next, next_plan));
                }
            }
        }
        next_frontier.sort_by(|a, b| {
            teacher_state_score(&b.0, content).total_cmp(&teacher_state_score(&a.0, content))
        });
        next_frontier.truncate(width);
        if next_frontier.is_empty() {
            break;
        }
        frontier = next_frontier;
    }
    best.or(fallback)
        .map_or_else(|| actions.into_iter().take(1).collect(), |(_, plan)| plan)
}

pub(in super::super) fn combat_search(
    game: &Game,
    content: &Content,
    width: usize,
    depth: usize,
) -> (Game, Vec<Action>) {
    let mut links: Vec<(Option<usize>, Action)> = Vec::new();
    let mut frontier = vec![(game.clone(), None)];
    let mut best = frontier[0].clone();
    let mut best_clear = None;
    let mut first_clear = None;
    for layer in 0..depth {
        let mut next = Vec::new();
        let mut cleared = Vec::new();
        for (state, tail) in frontier {
            for action in search_actions(&state, content)
                .into_iter()
                .filter(|action| {
                    let Some(combat) = state.combat() else {
                        return true;
                    };
                    let preserves_barrier = matches!(action, Action::Play { hand, .. }
                        if content.cards[combat.hand[*hand].id as usize].id == "CARD.SECOND_WIND"
                            && combat.hand.iter().any(|card|
                                content.cards[card.id as usize].id == "CARD.FLAME_BARRIER"));
                    let nemesis = combat.enemies.iter().any(|enemy| {
                        enemy.creature.hp > 0
                            && enemy.creature.powers.iter().any(|power| {
                                content.powers[power.id as usize].id == "POWER.NEMESIS_POWER"
                            })
                    });
                    !preserves_barrier || !nemesis
                })
            {
                let mut child = state.clone();
                if child.step(content, action.clone()).is_err() {
                    continue;
                }
                links.push((tail, action));
                let child_tail = Some(links.len() - 1);
                if child.combat().is_none() && !matches!(child.phase, Phase::Dead) {
                    cleared.push((child, child_tail));
                } else if !matches!(child.phase, Phase::Dead) {
                    next.push((child, child_tail));
                }
            }
        }
        if let Some(clear) = cleared.into_iter().max_by(|a, b| {
            teacher_state_score(&a.0, content).total_cmp(&teacher_state_score(&b.0, content))
        }) && best_clear
            .as_ref()
            .is_none_or(|best: &(Game, Option<usize>)| {
                teacher_state_score(&clear.0, content) > teacher_state_score(&best.0, content)
            })
        {
            best_clear = Some(clear);
            first_clear.get_or_insert(layer);
        }
        if first_clear.is_some_and(|first| layer >= first + 4) {
            let (child, tail) = best_clear.unwrap();
            return (child, collect_plan(&links, tail));
        }
        if next.is_empty() {
            if let Some((child, tail)) = best_clear {
                return (child, collect_plan(&links, tail));
            }
            break;
        }
        next.sort_by(|a, b| {
            teacher_state_score(&b.0, content).total_cmp(&teacher_state_score(&a.0, content))
        });
        let mut seen = HashSet::new();
        next.retain(|(game, _)| seen.insert(combat_key(game, content)));
        next.truncate(width);
        if best.1.is_none()
            || teacher_state_score(&next[0].0, content) > teacher_state_score(&best.0, content)
        {
            best = next[0].clone();
        }
        frontier = next;
    }
    if let Some((child, tail)) = best_clear {
        return (child, collect_plan(&links, tail));
    }
    (best.0, collect_plan(&links, best.1))
}

fn collect_plan(links: &[(Option<usize>, Action)], mut tail: Option<usize>) -> Vec<Action> {
    let mut plan = Vec::new();
    while let Some(index) = tail {
        plan.push(links[index].1.clone());
        tail = links[index].0;
    }
    plan.reverse();
    plan
}

fn extend_plan(
    links: &mut Vec<(Option<usize>, Action)>,
    mut tail: Option<usize>,
    actions: impl IntoIterator<Item = Action>,
) -> Option<usize> {
    for action in actions {
        links.push((tail, action));
        tail = Some(links.len() - 1);
    }
    tail
}

fn combat_key(game: &Game, content: &Content) -> (u16, i16, i16, i16, i32, i16, i16, bool, u64) {
    let combat = game.combat().unwrap();
    let mut enemy_hp = 0;
    let mut doom = 0;
    let mut enemies = 0u64;
    let detailed = game.room == Room::Boss;
    for enemy in &combat.enemies {
        enemy_hp += enemy.creature.hp.max(0) as i32;
        let mut state = (enemy.creature.id as u64) << 40
            | (enemy.creature.hp.max(0) as u64) << 16
            | (enemy.creature.block.max(0) as u64) << 4
            | enemy.move_index as u64;
        if detailed {
            state ^= (enemy.creature.max_hp.max(0) as u64).rotate_left(32);
            for power in &enemy.creature.powers {
                state ^= ((power.id as u64) << 32
                    | (power.amount as u16 as u64) << 16
                    | power.value as u16 as u64)
                    .rotate_left(power.id as u32 % 61);
            }
            for (index, &prior) in enemy.move_history.iter().enumerate() {
                state ^= ((prior as u64) << 8 | index as u64)
                    .rotate_left((index * 7 + prior) as u32 % 61);
            }
        }
        enemies ^= state.rotate_left(enemy.creature.id as u32 % 61);
        doom += enemy
            .creature
            .powers
            .iter()
            .filter(|power| content.powers[power.id as usize].id == "POWER.DOOM_POWER")
            .map(|power| power.amount)
            .sum::<i16>();
    }
    let countdown = combat
        .player
        .powers
        .iter()
        .filter(|power| content.powers[power.id as usize].id == "POWER.COUNTDOWN_POWER")
        .map(|power| power.amount)
        .sum();
    let reaper = combat.player.powers.iter().any(|power| {
        content.powers[power.id as usize].id == "POWER.REAPER_FORM_POWER" && power.amount > 0
    });
    let cards = |pile: &[Card], salt: u64| {
        pile.iter().enumerate().fold(salt, |hash, (index, card)| {
            hash ^ ((card.id as u64) << 24
                | (card.upgrades as u64) << 16
                | (card.value as u64) << 8
                | index as u64)
                .rotate_left((card.id as usize + index * 7) as u32 % 61)
        })
    };
    let player_powers = combat.player.powers.iter().fold(0, |hash, power| {
        let state = (power.id as u64) << 16
            | power.amount as u16 as u64
            | if detailed {
                (power.value as u16 as u64) << 32
            } else {
                0
            };
        hash ^ state.rotate_left(power.id as u32 % 61)
    });
    let history = if detailed {
        [
            combat.stars,
            combat.osty.hp,
            combat.history.attacks,
            combat.history.skills,
            combat.history.shivs,
            combat.history.stars_gained,
            combat.history.block_gains,
            combat.history.cards,
            combat.history.manual_cards,
            combat.history.manual_plays,
            combat.history.powers,
            combat.history.energy,
            combat.history.exhausted,
            combat.history.discarded,
            combat.history.generated,
            combat.history.ethereal,
            combat.history.extra_drawn,
            combat.history.doom_applied,
            combat.history.osty_attacks,
            combat.history.hp_lost,
            combat.history.hp_loss_events,
            combat.history.feral_returns,
            combat.max_energy,
            combat.draw_per_turn as i16,
            combat.orb_slots as i16,
        ]
        .iter()
        .enumerate()
        .fold(0, |hash, (index, value)| {
            hash ^ (*value as u16 as u64).rotate_left((index * 8) as u32)
        })
    } else {
        0
    };
    let orbs = if detailed {
        combat
            .orbs
            .iter()
            .enumerate()
            .fold(0, |hash, (index, orb)| {
                hash ^ ((orb.id as u64) << 16 | orb.value as u16 as u64)
                    .rotate_left((index * 11 + orb.id as usize) as u32 % 61)
            })
    } else {
        0
    };
    let resources = cards(&combat.hand, 0x4841_4e44)
        ^ cards(&combat.draw, 0x4452_4157)
        ^ cards(&combat.discard, 0x4449_5343)
        ^ cards(&combat.exhaust, 0x4558_4841)
        ^ player_powers
        ^ history
        ^ orbs;
    (
        combat.turn,
        combat.energy,
        combat.player.hp,
        combat.player.block,
        enemy_hp,
        doom,
        countdown,
        reaper,
        enemies ^ resources,
    )
}

fn room_key(game: &Game, content: &Content) -> (usize, u8, u8, u64) {
    let curses = game
        .run
        .deck
        .iter()
        .filter(|card| content.cards[card.id as usize].card_type == CardType::Curse)
        .count()
        .min(u8::MAX as usize) as u8;
    let engine = game.run.deck.iter().fold(0u64, |signature, card| {
        if !engine_card(game, content, *card) {
            return signature;
        }
        signature.wrapping_add(
            (card.id as u64 + 1)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                .rotate_left(card.upgrades as u32 % 64),
        )
    });
    (
        game.map.current.unwrap_or(usize::MAX),
        (game.run.hp.max(0) as i32 * 8 / game.run.max_hp.max(1) as i32) as u8,
        curses,
        engine,
    )
}

fn room_values(game: &Game, content: &Content) -> [i32; 8] {
    let key = room_key(game, content);
    [
        game.run.hp as i32,
        game.run.max_hp as i32,
        -(key.2 as i32),
        game.run.relics.len() as i32,
        game.run.gold,
        game.run.potions.iter().flatten().count() as i32,
        -(game.run.deck.len() as i32),
        game.run
            .deck
            .iter()
            .map(|card| engine_score(game, content, *card) as i32)
            .sum(),
    ]
}

pub(in super::super) fn retain_room_frontier<T>(
    states: &mut Vec<(Game, T)>,
    content: &Content,
    scalar_width: usize,
    width: usize,
) {
    let sort = |a: &(Game, T), b: &(Game, T)| {
        teacher_state_score(&b.0, content)
            .total_cmp(&teacher_state_score(&a.0, content))
            .then_with(|| room_key(&a.0, content).cmp(&room_key(&b.0, content)))
    };
    states.sort_by(sort);
    let mut pareto: Vec<(Game, T)> = Vec::new();
    for candidate in states.drain(..) {
        let key = room_key(&candidate.0, content);
        let values = room_values(&candidate.0, content);
        let dominates = |other: &(Game, T)| {
            let other_key = room_key(&other.0, content);
            other_key.0 == key.0
                && other_key.3 == key.3
                && room_values(&other.0, content)
                    .iter()
                    .zip(values)
                    .all(|(left, right)| *left >= right)
        };
        if pareto.iter().any(dominates) {
            continue;
        }
        pareto.retain(|other| {
            let other_key = room_key(&other.0, content);
            other_key.0 != key.0
                || other_key.3 != key.3
                || !values
                    .iter()
                    .zip(room_values(&other.0, content))
                    .all(|(left, right)| *left >= right)
        });
        pareto.push(candidate);
    }
    pareto.sort_by(sort);
    let scalar_width = scalar_width.min(width).min(pareto.len());
    let mut selected: Vec<(Game, T)> = pareto.drain(..scalar_width).collect();
    while selected.len() < width && !pareto.is_empty() {
        let index = (0..pareto.len())
            .max_by(|&a, &b| {
                let novelty = |game: &Game| {
                    let key = room_key(game, content);
                    4 * selected
                        .iter()
                        .all(|(other, _)| key.0 != room_key(other, content).0)
                        as u8
                        + 2 * selected
                            .iter()
                            .all(|(other, _)| key.3 != room_key(other, content).3)
                            as u8
                        + selected
                            .iter()
                            .all(|(other, _)| key.1 != room_key(other, content).1)
                            as u8
                        + selected
                            .iter()
                            .all(|(other, _)| key.2 != room_key(other, content).2)
                            as u8
                };
                novelty(&pareto[a].0)
                    .cmp(&novelty(&pareto[b].0))
                    .then_with(|| {
                        teacher_state_score(&pareto[a].0, content)
                            .total_cmp(&teacher_state_score(&pareto[b].0, content))
                    })
                    .then_with(|| {
                        room_key(&pareto[b].0, content).cmp(&room_key(&pareto[a].0, content))
                    })
            })
            .unwrap();
        selected.push(pareto.swap_remove(index));
    }
    selected.sort_by(sort);
    *states = selected;
}

pub(in super::super) fn search_actions(game: &Game, content: &Content) -> Vec<Action> {
    let mut actions = game.actions(content);
    if !game.replacing_potion
        && actions
            .iter()
            .any(|action| !matches!(action, Action::DiscardPotion(_)))
    {
        actions.retain(|action| !matches!(action, Action::DiscardPotion(_)));
    }
    actions
}

fn run_search(
    game: &Game,
    content: &Content,
    room_width: usize,
    combat_width: usize,
    combat_depth: usize,
    training_bonuses: (i16, i16),
) -> (Game, Vec<Action>) {
    let mut links: Vec<(Option<usize>, Action)> = Vec::new();
    let mut beam = vec![(game.clone(), None)];
    let mut best = beam[0].clone();
    let mut best_combat: Option<(Game, Option<usize>)> = None;
    for _ in 0..64 {
        let mut frontier = Vec::new();
        for (state, tail) in beam {
            for action in search_actions(&state, content) {
                let mut child = state.clone();
                let was_combat = child.combat().is_some();
                if child.step(content, action.clone()).is_ok() {
                    if !was_combat && child.combat().is_some() {
                        apply_training_bonuses(
                            &mut child,
                            content,
                            training_bonuses.0,
                            training_bonuses.1,
                        );
                    }
                    links.push((tail, action));
                    frontier.push((child, Some(links.len() - 1)));
                }
            }
        }
        let mut finished = Vec::new();
        for _ in 0..128 {
            let mut next = Vec::new();
            for (state, tail) in frontier {
                if matches!(state.phase, Phase::Won) {
                    return (state, collect_plan(&links, tail));
                }
                if matches!(state.phase, Phase::Map) {
                    finished.push((state, tail));
                } else if state.combat().is_some() {
                    let (child, path) = combat_search(&state, content, combat_width, combat_depth);
                    if child.combat().is_none() {
                        let child_tail = extend_plan(&mut links, tail, path);
                        next.push((child, child_tail));
                    } else if !matches!(child.phase, Phase::Dead) {
                        let replace = best_combat.as_ref().is_none_or(|(best, _)| {
                            (child.run.act, child.run.floor) > (best.run.act, best.run.floor)
                                || (child.run.act, child.run.floor)
                                    == (best.run.act, best.run.floor)
                                    && (combat_quality(&child) > combat_quality(best)
                                        || combat_quality(&child) == combat_quality(best)
                                            && teacher_state_score(&child, content)
                                                > teacher_state_score(best, content))
                        });
                        if replace {
                            best_combat = Some((child, extend_plan(&mut links, tail, path)));
                        }
                    }
                } else if !matches!(state.phase, Phase::Dead) {
                    for action in search_actions(&state, content) {
                        let mut child = state.clone();
                        let was_combat = child.combat().is_some();
                        if child.step(content, action.clone()).is_ok() {
                            if !was_combat && child.combat().is_some() {
                                apply_training_bonuses(
                                    &mut child,
                                    content,
                                    training_bonuses.0,
                                    training_bonuses.1,
                                );
                            }
                            links.push((tail, action));
                            next.push((child, Some(links.len() - 1)));
                        }
                    }
                }
            }
            retain_room_frontier(&mut next, content, room_width * 8, room_width * 8 + 1);
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        retain_room_frontier(&mut finished, content, room_width, room_width + 1);
        if finished.is_empty() {
            break;
        }
        if teacher_state_score(&finished[0].0, content) > teacher_state_score(&best.0, content) {
            best = finished[0].clone();
        }
        beam = finished;
    }
    let best = best_combat.unwrap_or(best);
    (best.0, collect_plan(&links, best.1))
}

#[pymethods]
impl Batch {
    #[new]
    #[pyo3(signature = (size=256, seed=1, character=None, teacher_width=32, teacher_turns=1, ascension=10, seed_stride=1))]
    fn new(
        size: usize,
        seed: u64,
        character: Option<Id>,
        teacher_width: usize,
        teacher_turns: usize,
        ascension: u8,
        seed_stride: u64,
    ) -> PyResult<Self> {
        let content = foundation_content();
        if character.is_some_and(|x| x as usize >= content.characters.len()) {
            return Err(PyValueError::new_err("invalid character"));
        }
        let layout = Layout::new(&content);
        let mut batch = Self {
            archive: vec![vec![]; content.characters.len() * 64 * PHASES],
            archive_seen: vec![0; content.characters.len() * 64 * PHASES],
            first_archive: vec![],
            actions: vec![vec![]; size],
            plans: vec![vec![]; size],
            starts: vec![],
            root_ids: vec![usize::MAX; size],
            games: Vec::with_capacity(size),
            random: seed.max(1),
            next_seed: seed,
            seed_stride: seed_stride.max(1),
            character,
            ascension,
            teacher_width: teacher_width.max(1),
            teacher_turns: teacher_turns.max(1),
            first_teacher: None,
            training_strength: 0,
            training_dexterity: 0,
            potential_weights: [0.0; POTENTIAL_WEIGHT_COUNT],
            resample_archive: true,
            archive_depth: 0,
            policy: None,
            searched_turns: vec![None; size],
            content,
            layout,
        };
        for _ in 0..size {
            let game = batch.fresh_game()?;
            batch.games.push(game);
        }
        Ok(batch)
    }

    #[pyo3(signature = (width=DEFAULT_MODEL_WIDTH, layers=DEFAULT_MODEL_LAYERS, heads=DEFAULT_MODEL_HEADS, feedforward=DEFAULT_MODEL_FEEDFORWARD))]
    fn token_layout(
        &self,
        width: usize,
        layers: usize,
        heads: usize,
        feedforward: usize,
    ) -> PyResult<std::collections::BTreeMap<String, usize>> {
        if !valid_model_shape(width, layers, heads, feedforward) {
            return Err(PyValueError::new_err("invalid model shape"));
        }
        let mut out = std::collections::BTreeMap::from([
            ("version".into(), VERSION as usize),
            ("characters".into(), self.layout.characters),
            ("globals".into(), globals_len(self.layout)),
            ("model_width".into(), width),
            ("model_layers".into(), layers),
            ("model_heads".into(), heads),
            ("model_feedforward".into(), feedforward),
            ("entity_summaries".into(), 0),
            ("base_state_width".into(), width),
            ("state_width".into(), width),
            ("action_width".into(), width),
            ("head_width".into(), width),
            ("domain_count".into(), DOMAIN_NAMES.len()),
            ("action_u".into(), ACTION_U),
            ("action_s".into(), ACTION_S),
            ("action_f".into(), ACTION_F),
            ("state_scope".into(), u32::MAX as usize),
            ("absent_node".into(), u32::MAX as usize),
            ("run_current_node".into(), 23),
            ("action_path_node".into(), 4),
            ("action_object".into(), 14),
            ("actor_owner".into(), 0),
            ("child_owner".into(), 0),
            ("child_kind".into(), 1),
            ("card_zone".into(), 0),
            ("card_order_kind".into(), 2),
            ("card_order".into(), 3),
            ("continuation_frame".into(), 2),
            ("continuation_parent".into(), 3),
            ("continuation_branch".into(), 4),
            ("continuation_path".into(), 5),
            ("continuation_list".into(), 7),
            ("continuation_order".into(), 8),
            ("map_node_id".into(), 0),
            ("map_node_floor".into(), 2),
            ("map_node_lane".into(), 3),
            ("map_node_topo".into(), 8),
            ("map_node_outdegree".into(), 9),
            ("map_edge_src".into(), 0),
            ("map_edge_dst".into(), 1),
            ("map_entry".into(), 0),
            ("player_owner".into(), 1),
            ("osty_owner".into(), 2),
            ("enemy_owner_start".into(), 3),
            ("card_zones".into(), CARD_ZONES),
            ("card_candidate_zone".into(), ATTACHED_CARD_ZONE),
            (
                "history_course_status".into(),
                HISTORY_COURSE_STATUS as usize,
            ),
            ("normal_encounters".into(), 58),
            ("elite_encounters".into(), 14),
            ("boss_encounters".into(), 14),
        ]);
        for (name, (u, s, c, f)) in DOMAIN_NAMES.into_iter().zip(DOMAIN_WIDTHS) {
            out.insert(format!("{name}_u"), u);
            out.insert(format!("{name}_s"), s);
            out.insert(format!("{name}_c"), c);
            out.insert(format!("{name}_f"), f);
        }
        out.insert("action_c".into(), ACTION_C);
        out.insert("concept_vocab".into(), self.layout.concept_vocab());
        for (index, name) in SEMANTIC_NAMES.into_iter().enumerate() {
            out.insert(
                format!("{name}_semantic_start"),
                self.layout.semantic_offsets[index] as usize,
            );
            out.insert(
                format!("{name}_semantic_count"),
                self.layout.semantic_sizes[index] as usize,
            );
        }
        Ok(out)
    }

    fn fingerprint(&self) -> u64 {
        content_fingerprint(&self.content)
    }

    fn load_snapshot(&mut self, snapshot: &str) -> PyResult<()> {
        let value: serde_json::Value = serde_json::from_str(snapshot)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let game = crate::replay::game_from_live_snapshot(
            &self.content,
            &value["before"],
            &value["oracle_before"],
        )
        .map_err(PyValueError::new_err)?;
        self.games = vec![game];
        self.actions = vec![vec![]];
        self.plans = vec![vec![]];
        self.starts.clear();
        self.root_ids = vec![usize::MAX];
        self.searched_turns = vec![None];
        Ok(())
    }

    fn load_snapshot_actions(&mut self, snapshot: &str) -> PyResult<()> {
        let value: serde_json::Value = serde_json::from_str(snapshot)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let game = crate::replay::game_from_live_snapshot(
            &self.content,
            &value["before"],
            &value["oracle_before"],
        )
        .map_err(PyValueError::new_err)?;
        let actions = game.actions(&self.content);
        self.games = actions
            .into_iter()
            .map(|action| {
                let mut next = game.clone();
                next.step(&self.content, action)
                    .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
                Ok(next)
            })
            .collect::<PyResult<_>>()?;
        self.actions = vec![vec![]; self.games.len()];
        self.plans = vec![vec![]; self.games.len()];
        self.starts.clear();
        self.root_ids = vec![usize::MAX; self.games.len()];
        self.searched_turns = vec![None; self.games.len()];
        Ok(())
    }

    fn action_descriptors(
        &self,
    ) -> PyResult<Vec<(String, Option<String>, Option<String>, Option<String>)>> {
        let game = self
            .games
            .first()
            .ok_or_else(|| PyValueError::new_err("empty environment"))?;
        Ok(game
            .actions(&self.content)
            .iter()
            .map(|action| action_descriptor(game, &self.content, action))
            .collect())
    }

    fn action_previews(&self) -> PyResult<Vec<Option<String>>> {
        let game = self
            .games
            .first()
            .ok_or_else(|| PyValueError::new_err("empty environment"))?;
        Ok(game
            .actions(&self.content)
            .iter()
            .map(|action| action_preview(game, &self.content, action))
            .collect())
    }

    fn seeds(&self) -> Vec<u32> {
        self.games.iter().map(|game| game.seed).collect()
    }

    fn characters(&self) -> Vec<Id> {
        self.games.iter().map(|game| game.run.character).collect()
    }

    fn state(&self, index: usize) -> PyResult<String> {
        let game = self
            .games
            .get(index)
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
        Ok(game_state(game, &self.content).to_string())
    }

    fn root_ids(&self) -> Vec<usize> {
        self.root_ids.clone()
    }

    fn deck(&self, index: usize) -> PyResult<Vec<String>> {
        let game = self
            .games
            .get(index)
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
        Ok(game
            .run
            .deck
            .iter()
            .map(|card| self.content.cards[card.id as usize].id.to_owned())
            .collect())
    }

    fn set_training_bonus(&mut self, bonus: i16) {
        let bonus = bonus.max(0);
        self.set_training_bonuses(bonus, bonus / 2);
    }

    fn set_training_bonuses(&mut self, strength: i16, dexterity: i16) {
        let bonuses = (strength.max(0), dexterity.max(0));
        let previous = (self.training_strength, self.training_dexterity);
        if bonuses == previous {
            return;
        }
        for game in self.games.iter_mut().chain(&mut self.starts) {
            apply_training_bonuses(game, &self.content, -previous.0, -previous.1);
            apply_training_bonuses(game, &self.content, bonuses.0, bonuses.1);
        }
        (self.training_strength, self.training_dexterity) = bonuses;
        self.plans.iter_mut().for_each(Vec::clear);
    }

    fn set_potential_weights(&mut self, weights: Vec<f32>) -> PyResult<()> {
        if weights.len() != POTENTIAL_WEIGHT_COUNT
            || weights.iter().any(|weight| !weight.is_finite())
        {
            return Err(PyValueError::new_err(
                "potential weights must be 13 finite numbers",
            ));
        }
        self.potential_weights = weights.try_into().unwrap();
        Ok(())
    }

    fn set_archive_resampling(&mut self, enabled: bool) {
        self.resample_archive = enabled;
    }

    fn set_archive_depth(&mut self, depth: usize) {
        self.archive_depth = depth;
    }

    #[pyo3(signature = (seed, choices, character=None))]
    fn archive_path(
        &mut self,
        seed: u64,
        choices: Vec<usize>,
        character: Option<Id>,
    ) -> PyResult<bool> {
        let character = character.or(self.character).unwrap_or(0);
        if character as usize >= self.content.characters.len() {
            return Err(PyValueError::new_err("invalid character"));
        }
        let mut game =
            Game::new_character_ascension(&self.content, seed, character, self.ascension)
                .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
        game.begin_run(&self.content)
            .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
        for choice in choices {
            let was_combat = game.combat().is_some();
            let action = game
                .actions(&self.content)
                .get(choice)
                .cloned()
                .ok_or_else(|| PyValueError::new_err("invalid archived action"))?;
            game.step(&self.content, action.clone())
                .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
            if matches!(action, Action::Path(_)) {
                let floor = (game.run.act.saturating_sub(1) as usize * 18
                    + game.run.floor as usize)
                    .min(63);
                let key =
                    (game.run.character as usize * 64 + floor) * PHASES + phase_index(&game.phase);
                if self.archive[key].len() < 16 {
                    self.archive[key].push(game.clone());
                }
            }
            if !was_combat && game.combat().is_some() {
                apply_training_bonuses(
                    &mut game,
                    &self.content,
                    self.training_strength,
                    self.training_dexterity,
                );
            }
            if matches!(game.phase, Phase::Won | Phase::Dead) {
                break;
            }
        }
        Ok(matches!(game.phase, Phase::Won))
    }

    fn set_teacher(&mut self, width: usize, turns: usize) {
        self.teacher_width = width.max(1);
        self.teacher_turns = turns.max(1);
        self.plans.iter_mut().for_each(Vec::clear);
    }

    fn set_first_teacher(&mut self, width: usize, turns: usize) {
        self.first_teacher = Some((width.max(1), turns.max(1)));
        self.plans.first_mut().into_iter().for_each(Vec::clear);
    }

    fn mark_starts(&mut self) {
        self.starts.clone_from(&self.games);
    }

    fn repeat_exact(&mut self, index: usize) -> PyResult<()> {
        let game = self
            .games
            .get(index)
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?
            .clone();
        self.games.fill(game);
        self.actions.iter_mut().for_each(Vec::clear);
        self.plans.iter_mut().for_each(Vec::clear);
        Ok(())
    }

    fn pair_exact(&mut self) {
        self.games.par_chunks_mut(2).for_each(|pair| {
            if pair.len() == 2 {
                pair[1] = pair[0].clone();
            }
        });
        self.actions.iter_mut().for_each(Vec::clear);
        self.plans.iter_mut().for_each(Vec::clear);
    }

    fn repeat_groups(&mut self, size: usize) -> PyResult<()> {
        if size == 0 || !self.games.len().is_multiple_of(size) {
            return Err(PyValueError::new_err("invalid group size"));
        }
        self.games.par_chunks_mut(size).for_each(|group| {
            let game = group[0].clone();
            group[1..].fill(game);
        });
        self.actions.iter_mut().for_each(Vec::clear);
        self.plans.iter_mut().for_each(Vec::clear);
        Ok(())
    }

    #[pyo3(signature = (index=0, repeats=1, seed=None))]
    fn repeat_resampled(
        &mut self,
        index: usize,
        repeats: usize,
        seed: Option<u64>,
    ) -> PyResult<()> {
        let base = self
            .games
            .get(index)
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?
            .clone();
        let repeats = repeats.max(1);
        let mut random = seed.map(Rng::from_seed);
        let mut particle = 0;
        let seeds = (0..self.games.len())
            .map(|index| {
                if index % repeats == 0 {
                    particle = random.as_mut().map_or_else(|| self.random_u64(), Rng::next);
                }
                particle
            })
            .collect::<Vec<_>>();
        let content = &self.content;
        self.games
            .par_iter_mut()
            .zip(seeds)
            .for_each(|(game, seed)| {
                *game = base.clone();
                resample_hidden(game, content, seed);
            });
        self.actions.iter_mut().for_each(Vec::clear);
        self.plans.iter_mut().for_each(Vec::clear);
        Ok(())
    }

    #[pyo3(signature = (source, indices, repeats=1, seed=None))]
    fn copy_resampled(
        &mut self,
        source: PyRef<'_, Batch>,
        indices: Vec<usize>,
        repeats: usize,
        seed: Option<u64>,
    ) -> PyResult<()> {
        if repeats == 0 || indices.len() * repeats != self.games.len() {
            return Err(PyValueError::new_err("invalid repeat count"));
        }
        let roots = indices
            .iter()
            .map(|&index| {
                source
                    .games
                    .get(index)
                    .cloned()
                    .ok_or_else(|| PyValueError::new_err("invalid environment index"))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let mut random = seed.map(Rng::from_seed);
        let seeds = (0..self.games.len())
            .map(|_| random.as_mut().map_or_else(|| self.random_u64(), Rng::next))
            .collect::<Vec<_>>();
        let content = &self.content;
        self.games
            .par_iter_mut()
            .zip(seeds)
            .enumerate()
            .for_each(|(index, (game, seed))| {
                *game = roots[index / repeats].clone();
                resample_hidden(game, content, seed);
            });
        self.training_strength = source.training_strength;
        self.training_dexterity = source.training_dexterity;
        self.actions.iter_mut().for_each(Vec::clear);
        self.plans.iter_mut().for_each(Vec::clear);
        self.starts.clear();
        self.root_ids.fill(usize::MAX);
        self.searched_turns.fill(None);
        Ok(())
    }

    #[pyo3(signature = (depth=0))]
    fn repeat_best(&mut self, depth: usize) {
        let character = self.character.unwrap_or(self.games[0].run.character) as usize;
        let mut floors = self
            .archive
            .iter()
            .enumerate()
            .filter(|(key, games)| !games.is_empty() && *key / (64 * PHASES) == character)
            .map(|(key, _)| key / PHASES % 64)
            .collect::<Vec<_>>();
        floors.sort_unstable();
        floors.dedup();
        let Some(&floor) = floors.iter().rev().nth(depth) else {
            return;
        };
        let mut roots = self
            .archive
            .iter()
            .enumerate()
            .filter(|(key, _)| *key / (64 * PHASES) == character && *key / PHASES % 64 == floor)
            .flat_map(|(_, games)| games)
            .cloned()
            .collect::<Vec<_>>();
        roots.sort_by(|a, b| {
            teacher_state_score(b, &self.content).total_cmp(&teacher_state_score(a, &self.content))
        });
        if !roots.is_empty() {
            let seeds = (0..self.games.len())
                .map(|_| self.random_u64())
                .collect::<Vec<_>>();
            for (index, (game, seed)) in self.games.iter_mut().zip(seeds).enumerate() {
                *game = roots[index % roots.len()].clone();
                if self.resample_archive {
                    resample_hidden(game, &self.content, seed);
                }
                apply_training_bonuses(
                    game,
                    &self.content,
                    self.training_strength,
                    self.training_dexterity,
                );
            }
            self.actions.iter_mut().for_each(Vec::clear);
            self.plans.iter_mut().for_each(Vec::clear);
        }
    }

    fn restore(&mut self) -> PyResult<()> {
        if self.starts.len() != self.games.len() {
            return Err(PyValueError::new_err("mark starts before restore"));
        }
        self.games.clone_from(&self.starts);
        self.actions.iter_mut().for_each(Vec::clear);
        self.plans.iter_mut().for_each(Vec::clear);
        Ok(())
    }

    #[pyo3(signature = (active=None))]
    fn teacher(&mut self, active: Option<Vec<bool>>) -> PyResult<Vec<usize>> {
        let active = active.unwrap_or_else(|| vec![true; self.games.len()]);
        if active.len() != self.games.len() {
            return Err(PyValueError::new_err("invalid active mask"));
        }
        Ok(self
            .games
            .par_iter()
            .zip(self.plans.par_iter_mut())
            .zip(&self.actions)
            .zip(active)
            .enumerate()
            .map(|(index, (((game, plan), represented), active))| {
                if !active {
                    return 0;
                }
                let actions = game.actions(&self.content);
                if plan.first().is_none_or(|action| !actions.contains(action)) {
                    let (width, turns) = if index == 0 {
                        self.first_teacher
                            .unwrap_or((self.teacher_width, self.teacher_turns))
                    } else {
                        (self.teacher_width, self.teacher_turns)
                    };
                    *plan = teacher_plan(game, &self.content, width, turns);
                }
                let Some(action) = plan.first().cloned() else {
                    return 0;
                };
                represented
                    .iter()
                    .position(|candidate| *candidate == action)
                    .unwrap_or(0)
            })
            .collect())
    }

    fn teacher_first(&self) -> usize {
        let game = &self.games[0];
        let actions = game.actions(&self.content);
        let (width, turns) = self
            .first_teacher
            .unwrap_or((self.teacher_width, self.teacher_turns));
        teacher_plan(game, &self.content, width, turns)
            .first()
            .and_then(|action| actions.iter().position(|legal| legal == action))
            .unwrap_or(0)
    }

    #[pyo3(signature = (choices, danger=1.0, tactical=false))]
    fn rescue(&self, choices: Vec<usize>, danger: f32, tactical: bool) -> PyResult<Vec<usize>> {
        if choices.len() != self.games.len() {
            return Err(PyValueError::new_err("invalid choices"));
        }
        Ok(self
            .games
            .par_iter()
            .zip(choices)
            .map(|(game, choice)| rescue_choice(game, &self.content, choice, danger, tactical))
            .collect())
    }

    #[pyo3(signature = (index, width=64, turns=3))]
    fn teacher_at(&self, index: usize, width: usize, turns: usize) -> PyResult<usize> {
        let game = self
            .games
            .get(index)
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
        let actions = game.actions(&self.content);
        Ok(teacher_plan(game, &self.content, width, turns)
            .first()
            .and_then(|action| actions.iter().position(|legal| legal == action))
            .unwrap_or(0))
    }

    #[pyo3(signature = (index, width=1024, depth=128))]
    fn search_at(
        &self,
        index: usize,
        width: usize,
        depth: usize,
    ) -> PyResult<(bool, Vec<usize>, f32)> {
        let game = self
            .games
            .get(index)
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
        let (result, plan) = combat_search(game, &self.content, width.max(1), depth.max(1));
        let mut replay = game.clone();
        let mut choices = Vec::with_capacity(plan.len());
        for action in plan {
            let actions = replay.actions(&self.content);
            choices.push(actions.iter().position(|legal| *legal == action).unwrap());
            replay.step(&self.content, action).unwrap();
        }
        Ok((
            result.combat().is_none() && !matches!(result.phase, Phase::Dead),
            choices,
            teacher_state_score(&result, &self.content),
        ))
    }

    #[pyo3(signature = (width=64, depth=160))]
    fn search_scores(&self, width: usize, depth: usize) -> Vec<(bool, f32)> {
        self.games
            .par_iter()
            .map(|game| {
                let (result, _) = if game.combat().is_some() {
                    combat_search(game, &self.content, width.max(1), depth.max(1))
                } else {
                    (game.clone(), Vec::new())
                };
                (
                    result.combat().is_none() && !matches!(result.phase, Phase::Dead),
                    combat_quality(&result),
                )
            })
            .collect()
    }

    fn run_scores(
        &self,
        room_width: usize,
        combat_width: usize,
        combat_depth: usize,
    ) -> Vec<(bool, f32)> {
        self.games
            .par_iter()
            .map(|game| {
                let (result, _) = run_search(
                    game,
                    &self.content,
                    room_width.max(1),
                    combat_width.max(1),
                    combat_depth.max(1),
                    (self.training_strength, self.training_dexterity),
                );
                (
                    matches!(result.phase, Phase::Won),
                    teacher_state_score(&result, &self.content),
                )
            })
            .collect()
    }

    #[pyo3(signature = (width=64, depth=160))]
    fn search_all(&self, width: usize, depth: usize) -> Vec<(bool, Vec<usize>, f32)> {
        self.games
            .par_iter()
            .map(|game| {
                let (result, plan) = combat_search(game, &self.content, width.max(1), depth.max(1));
                let mut replay = game.clone();
                let choices = plan
                    .into_iter()
                    .map(|action| {
                        let actions = replay.actions(&self.content);
                        let choice = actions.iter().position(|legal| *legal == action).unwrap();
                        replay.step(&self.content, action).unwrap();
                        choice
                    })
                    .collect();
                (
                    result.combat().is_none() && !matches!(result.phase, Phase::Dead),
                    choices,
                    combat_quality(&result),
                )
            })
            .collect()
    }

    #[pyo3(signature = (index, room_width=4, combat_width=128, combat_depth=160))]
    fn run_search_at(
        &self,
        index: usize,
        room_width: usize,
        combat_width: usize,
        combat_depth: usize,
    ) -> PyResult<(bool, Vec<usize>, f32)> {
        let game = self
            .games
            .get(index)
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
        let (result, plan) = run_search(
            game,
            &self.content,
            room_width.max(1),
            combat_width.max(1),
            combat_depth.max(1),
            (self.training_strength, self.training_dexterity),
        );
        let mut replay = game.clone();
        let mut choices = Vec::with_capacity(plan.len());
        for action in plan {
            let actions = replay.actions(&self.content);
            choices.push(actions.iter().position(|legal| *legal == action).unwrap());
            let was_combat = replay.combat().is_some();
            replay.step(&self.content, action).unwrap();
            if !was_combat && replay.combat().is_some() {
                apply_training_bonuses(
                    &mut replay,
                    &self.content,
                    self.training_strength,
                    self.training_dexterity,
                );
            }
        }
        Ok((
            matches!(result.phase, Phase::Won),
            choices,
            teacher_state_score(&result, &self.content),
        ))
    }

    #[pyo3(signature = (index, source_bonus, choices, room_width=2, combat_width=64, combat_depth=160, source_dexterity=None))]
    fn repair_path_at(
        &self,
        index: usize,
        source_bonus: i16,
        choices: Vec<usize>,
        room_width: usize,
        combat_width: usize,
        combat_depth: usize,
        source_dexterity: Option<i16>,
    ) -> PyResult<(bool, Vec<usize>, f32)> {
        let root = self
            .games
            .get(index)
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
        let mut source = root.clone();
        let source_bonus = source_bonus.max(0);
        let source_dexterity = source_dexterity.unwrap_or(source_bonus / 2).max(0);
        if source.combat().is_some() {
            apply_training_bonuses(
                &mut source,
                &self.content,
                -self.training_strength,
                -self.training_dexterity,
            );
            apply_training_bonuses(&mut source, &self.content, source_bonus, source_dexterity);
        }
        let mut route = Vec::with_capacity(choices.len());
        for choice in choices {
            let action = source
                .actions(&self.content)
                .get(choice)
                .cloned()
                .ok_or_else(|| PyValueError::new_err("invalid source path"))?;
            let combat = source.combat().is_some();
            let features = (!combat).then(|| {
                candidate_signature(&source, &self.content, self.layout, &action).unwrap()
            });
            source
                .step(&self.content, action.clone())
                .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
            if !combat && source.combat().is_some() {
                apply_training_bonuses(&mut source, &self.content, source_bonus, source_dexterity);
            }
            route.push((action, combat, features));
        }

        let mut target = root.clone();
        let mut repaired = Vec::new();
        let append = |game: &mut Game, path: &mut Vec<usize>, action: Action| {
            let choice = game
                .actions(&self.content)
                .iter()
                .position(|candidate| *candidate == action)
                .ok_or_else(|| PyValueError::new_err("repaired action is not legal"))?;
            let combat = game.combat().is_some();
            game.step(&self.content, action)
                .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
            if !combat && game.combat().is_some() {
                apply_training_bonuses(
                    game,
                    &self.content,
                    self.training_strength,
                    self.training_dexterity,
                );
            }
            path.push(choice);
            Ok::<(), PyErr>(())
        };
        let mut cursor = 0;
        let mut fallback = false;
        loop {
            if target.combat().is_some() {
                let (result, plan) = combat_search(
                    &target,
                    &self.content,
                    combat_width.max(1),
                    combat_depth.max(1),
                );
                for action in plan {
                    append(&mut target, &mut repaired, action)?;
                }
                while cursor < route.len() && route[cursor].1 {
                    cursor += 1;
                }
                if result.combat().is_some() || matches!(result.phase, Phase::Dead) {
                    break;
                }
                continue;
            }
            while cursor < route.len() && route[cursor].1 {
                cursor += 1;
            }
            if cursor == route.len() || matches!(target.phase, Phase::Won | Phase::Dead) {
                break;
            }
            let (action, _, features) = &route[cursor];
            let matches = target.actions(&self.content).iter().any(|candidate| {
                candidate == action
                    && features.as_ref().is_some_and(|features| {
                        candidate_signature(&target, &self.content, self.layout, candidate)
                            .is_some_and(|candidate| candidate == *features)
                    })
            });
            if !matches {
                fallback = true;
                break;
            }
            append(&mut target, &mut repaired, action.clone())?;
            cursor += 1;
        }
        if fallback {
            let (_, suffix) = run_search(
                &target,
                &self.content,
                room_width.max(1),
                combat_width.max(1),
                combat_depth.max(1),
                (self.training_strength, self.training_dexterity),
            );
            for action in suffix {
                append(&mut target, &mut repaired, action)?;
            }
        }
        Ok((
            matches!(target.phase, Phase::Won),
            repaired,
            teacher_state_score(&target, &self.content),
        ))
    }

    #[pyo3(signature = (room_width=2, combat_width=64, combat_depth=160, count=None))]
    fn run_search_all(
        &self,
        room_width: usize,
        combat_width: usize,
        combat_depth: usize,
        count: Option<usize>,
    ) -> Vec<(u32, bool, Vec<usize>, f32)> {
        self.games[..count.unwrap_or(self.games.len()).min(self.games.len())]
            .par_iter()
            .map(|game| {
                let (result, plan) = run_search(
                    game,
                    &self.content,
                    room_width.max(1),
                    combat_width.max(1),
                    combat_depth.max(1),
                    (self.training_strength, self.training_dexterity),
                );
                let mut replay = game.clone();
                let choices = plan
                    .into_iter()
                    .map(|action| {
                        let actions = replay.actions(&self.content);
                        let choice = actions.iter().position(|legal| *legal == action).unwrap();
                        let was_combat = replay.combat().is_some();
                        replay.step(&self.content, action).unwrap();
                        if !was_combat && replay.combat().is_some() {
                            apply_training_bonuses(
                                &mut replay,
                                &self.content,
                                self.training_strength,
                                self.training_dexterity,
                            );
                        }
                        choice
                    })
                    .collect();
                (
                    game.seed,
                    matches!(result.phase, Phase::Won),
                    choices,
                    teacher_state_score(&result, &self.content),
                )
            })
            .collect()
    }

    fn stats(&self) -> Vec<(u8, u8, i16, i16, u8, u16, i16, i32, f32, u8)> {
        let content = &self.content;
        self.games
            .iter()
            .map(|game| {
                let (turn, player_hp, enemy_hp) = game.combat().map_or((0, 0, 0), |combat| {
                    (
                        combat.turn,
                        combat.player.hp,
                        combat
                            .enemies
                            .iter()
                            .map(|enemy| {
                                let future = if content.enemies[enemy.creature.id as usize].id
                                    == "MONSTER.TEST_SUBJECT"
                                    && enemy.creature.powers.iter().any(|power| {
                                        content.powers[power.id as usize].id
                                            == "POWER.ADAPTABLE_POWER"
                                    }) {
                                    if enemy.creature.max_hp < 200 {
                                        525
                                    } else {
                                        313
                                    }
                                } else {
                                    0
                                };
                                enemy.creature.hp.max(0) as i32 + future
                            })
                            .sum(),
                    )
                });
                (
                    game.run.act,
                    game.run.floor,
                    game.run.hp,
                    game.run.max_hp,
                    phase_index(&game.phase) as u8,
                    turn,
                    player_hp,
                    enemy_hp,
                    teacher_state_score(game, content),
                    canonical_progress(game),
                )
            })
            .collect()
    }

    fn describe(&self, index: usize, choice: usize) -> PyResult<String> {
        let game = self
            .games
            .get(index)
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
        let actions = game.actions(&self.content);
        let action = actions
            .get(choice)
            .ok_or_else(|| PyValueError::new_err("invalid action index"))?;
        let card = match action {
            Action::Play { hand, .. } => game
                .combat()
                .and_then(|combat| combat.hand.get(*hand))
                .map(|card| self.content.cards[card.id as usize].id),
            Action::RewardCard(index) => match &game.phase {
                Phase::Rewards(rewards) => rewards
                    .cards
                    .get(*index)
                    .map(|card| self.content.cards[card.id as usize].id),
                _ => None,
            },
            Action::Buy(index) => match &game.phase {
                Phase::Shop(items) => items.get(*index).and_then(|item| match item {
                    ShopItem::Card(card, _) => Some(self.content.cards[card.id as usize].id),
                    _ => None,
                }),
                _ => None,
            },
            _ => None,
        };
        let combat = game.combat().map_or_else(String::new, |combat| {
            format!(
                " turn={} energy={} player={}/{} block={} enemies={:?}",
                combat.turn,
                combat.energy,
                combat.player.hp,
                combat.player.max_hp,
                combat.player.block,
                combat
                    .enemies
                    .iter()
                    .map(|enemy| (
                        self.content.enemies[enemy.creature.id as usize].id,
                        enemy.creature.hp,
                        enemy.creature.block,
                        enemy.move_index,
                    ))
                    .collect::<Vec<_>>()
            )
        });
        Ok(format!(
            "act={} floor={} hp={}/{} phase={}{} action={action:?} card={card:?}",
            game.run.act,
            game.run.floor,
            game.run.hp,
            game.run.max_hp,
            phase_index(&game.phase),
            combat,
        ))
    }

    fn run_summary(&self, index: usize) -> PyResult<String> {
        let game = self
            .games
            .get(index)
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
        Ok(format!(
            "act={} floor={} hp={}/{} gold={} relics={:?} deck={:?}",
            game.run.act,
            game.run.floor,
            game.run.hp,
            game.run.max_hp,
            game.run.gold,
            game.run
                .relics
                .iter()
                .map(|id| self.content.relics[*id as usize].id)
                .collect::<Vec<_>>(),
            game.run
                .deck
                .iter()
                .map(|card| (self.content.cards[card.id as usize].id, card.upgrades,))
                .collect::<Vec<_>>(),
        ))
    }

    #[pyo3(signature = (active=None, flat=false))]
    fn observe_tokens<'py>(
        &mut self,
        py: Python<'py>,
        active: Option<Vec<bool>>,
        flat: bool,
    ) -> PyResult<Bound<'py, PyTuple>> {
        let layout = self.layout;
        let content = &self.content;
        let bonuses = (self.training_strength, self.training_dexterity);
        let potential_weights = self.potential_weights;
        let active = active.unwrap_or_else(|| vec![true; self.games.len()]);
        if active.len() != self.games.len() {
            return Err(PyValueError::new_err("invalid active mask"));
        }
        let rows = py.allow_threads(|| {
            self.games
                .par_iter()
                .zip(&active)
                .map(|(game, &active)| {
                    if !active {
                        return None;
                    }
                    debug_assert!(game.combat().is_none_or(|combat| {
                        combat.queue.is_empty() || combat.choice.is_some()
                    }));
                    Some(observation_v56(game, content, layout, bonuses))
                })
                .collect::<Vec<_>>()
        });
        let batches = rows.len();
        let mut characters = vec![u8::MAX; batches];
        let mut globals = vec![0.0; batches * globals_len(layout)];
        let mut potentials = vec![0.0; batches];
        let mut digests = vec![0u64; batches];
        let max_actions = rows
            .iter()
            .filter_map(Option::as_ref)
            .map(|row| row.candidates.len())
            .max()
            .unwrap_or(0)
            .max(1);
        let padded_actions = if flat { 0 } else { batches * max_actions };
        let mut action_u = vec![0; padded_actions * ACTION_U];
        let mut action_s = vec![0; padded_actions * ACTION_S];
        let mut action_c = vec![0; padded_actions * ACTION_C];
        let mut action_f = vec![0.0; padded_actions * ACTION_F];
        let mut represented = vec![0; padded_actions];
        let mut legal = vec![0; batches * max_actions];
        for (batch, row) in rows.iter().enumerate() {
            let Some(row) = row else {
                continue;
            };
            characters[batch] = row.character;
            globals[batch * globals_len(layout)..(batch + 1) * globals_len(layout)]
                .copy_from_slice(&row.globals);
            potentials[batch] = potential_value(&row.potential, &potential_weights);
            for (position, candidate) in row.candidates.iter().enumerate() {
                let base = batch * max_actions + position;
                if !flat {
                    action_u[base * ACTION_U..(base + 1) * ACTION_U].copy_from_slice(&candidate.u);
                    action_s[base * ACTION_S..(base + 1) * ACTION_S].copy_from_slice(&candidate.s);
                    action_c[base * ACTION_C..(base + 1) * ACTION_C].copy_from_slice(&candidate.c);
                    action_f[base * ACTION_F..(base + 1) * ACTION_F].copy_from_slice(&candidate.f);
                    represented[base] = 1;
                }
                legal[base] = candidate.legal as u8;
            }
        }
        if flat {
            let packed_data = py.allow_threads(|| {
                rows.par_iter()
                    .map(|row| {
                        row.as_ref().map(|row| {
                            let (globals, counts, exact, actions, digest) = packed_observation(row);
                            (
                                row.character,
                                globals,
                                counts,
                                compress_words(&exact),
                                actions,
                                digest,
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            });
            let mut packed_domains = Vec::with_capacity(DOMAIN_NAMES.len());
            let action_offsets = rows
                .iter()
                .scan(0usize, |offset, row| {
                    let start = *offset;
                    *offset += row.as_ref().map_or(0, |row| row.candidates.len());
                    Some(start)
                })
                .collect::<Vec<_>>();
            for (domain, &(u_width, s_width, c_width, f_width)) in DOMAIN_WIDTHS.iter().enumerate()
            {
                let total = rows
                    .iter()
                    .filter_map(Option::as_ref)
                    .map(|row| row.domains[domain].len())
                    .sum();
                let mut exact_u: Vec<u32> = Vec::with_capacity(total * u_width);
                let mut exact_s: Vec<i32> = Vec::with_capacity(total * s_width);
                let mut semantic: Vec<u32> = Vec::with_capacity(total * c_width);
                let mut numeric: Vec<f32> = Vec::with_capacity(total * f_width);
                let mut scope: Vec<i32> = Vec::with_capacity(total);
                let mut row_index: Vec<i32> = Vec::with_capacity(total);
                for (batch, row) in rows.iter().enumerate() {
                    let Some(row) = row else { continue };
                    for record in &row.domains[domain] {
                        exact_u.extend(&record.u);
                        exact_s.extend(&record.s);
                        semantic.extend(&record.c);
                        numeric.extend(&record.f);
                        scope.push(if record.scope < 0 {
                            record.scope
                        } else {
                            record.scope + action_offsets[batch] as i32
                        });
                        row_index.push(batch as i32);
                    }
                }
                packed_domains.push(
                    PyTuple::new(
                        py,
                        [
                            ndarray::Array2::from_shape_vec((total, u_width), exact_u)
                                .unwrap()
                                .into_pyarray(py)
                                .into_any(),
                            ndarray::Array2::from_shape_vec((total, s_width), exact_s)
                                .unwrap()
                                .into_pyarray(py)
                                .into_any(),
                            ndarray::Array2::from_shape_vec((total, c_width), semantic)
                                .unwrap()
                                .into_pyarray(py)
                                .into_any(),
                            ndarray::Array2::from_shape_vec((total, f_width), numeric)
                                .unwrap()
                                .into_pyarray(py)
                                .into_any(),
                            ndarray::Array1::from_vec(row_index)
                                .into_pyarray(py)
                                .into_any(),
                            ndarray::Array1::from_vec(scope).into_pyarray(py).into_any(),
                        ],
                    )?
                    .into_any(),
                );
            }
            let total_actions = action_offsets.last().copied().unwrap_or(0)
                + rows
                    .last()
                    .and_then(Option::as_ref)
                    .map_or(0, |row| row.candidates.len());
            let mut flat_u: Vec<u32> = Vec::with_capacity(total_actions * ACTION_U);
            let mut flat_s: Vec<i32> = Vec::with_capacity(total_actions * ACTION_S);
            let mut flat_c: Vec<u32> = Vec::with_capacity(total_actions * ACTION_C);
            let mut flat_f: Vec<f32> = Vec::with_capacity(total_actions * ACTION_F);
            let mut action_row: Vec<i32> = Vec::with_capacity(total_actions);
            let mut action_position: Vec<i32> = Vec::with_capacity(total_actions);
            for (batch, row) in rows.iter().enumerate() {
                let Some(row) = row else { continue };
                for (position, candidate) in row.candidates.iter().enumerate() {
                    flat_u.extend(candidate.u);
                    flat_s.extend(candidate.s);
                    flat_c.extend(candidate.c);
                    flat_f.extend(candidate.f);
                    action_row.push(batch as i32);
                    action_position.push(position as i32);
                }
            }
            let actions = PyTuple::new(
                py,
                [
                    ndarray::Array2::from_shape_vec((total_actions, ACTION_U), flat_u)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array2::from_shape_vec((total_actions, ACTION_S), flat_s)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array2::from_shape_vec((total_actions, ACTION_C), flat_c)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array2::from_shape_vec((total_actions, ACTION_F), flat_f)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array1::from_vec(action_row)
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array1::from_vec(action_position)
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array2::from_shape_vec((batches, max_actions), legal)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                ],
            )?;
            let mut packed_rows = Vec::with_capacity(batches);
            for (batch, row) in packed_data.into_iter().enumerate() {
                let (character, globals, counts, exact, actions, digest) =
                    row.unwrap_or_else(|| {
                        (
                            u8::MAX,
                            vec![0.0; globals_len(layout)],
                            vec![0; DOMAIN_NAMES.len()],
                            Vec::new(),
                            Vec::new(),
                            0,
                        )
                    });
                digests[batch] = digest;
                packed_rows.push(PyTuple::new(
                    py,
                    [
                        character.into_pyobject(py)?.into_any(),
                        ndarray::Array1::from_vec(globals)
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array1::from_vec(counts)
                            .into_pyarray(py)
                            .into_any(),
                        PyBytes::new(py, &exact).into_any(),
                        ndarray::Array2::from_shape_vec(
                            (
                                actions.len() / (ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1),
                                ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1,
                            ),
                            actions,
                        )
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                        digest.into_pyobject(py)?.into_any(),
                    ],
                )?);
            }
            self.actions = rows
                .iter()
                .map(|row| {
                    row.as_ref().map_or_else(Vec::new, |row| {
                        row.candidates
                            .iter()
                            .map(|candidate| candidate.action.clone())
                            .collect()
                    })
                })
                .collect();
            return PyTuple::new(
                py,
                [
                    ndarray::Array1::from_vec(characters)
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array2::from_shape_vec((batches, globals_len(layout)), globals)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                    PyTuple::new(py, packed_domains)?.into_any(),
                    actions.into_any(),
                    ndarray::Array1::from_vec(potentials)
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array1::from_vec(digests)
                        .into_pyarray(py)
                        .into_any(),
                    PyTuple::new(py, packed_rows)?.into_any(),
                ],
            );
        }
        let mut packed_domains = Vec::with_capacity(DOMAIN_NAMES.len());
        for (domain, &(u_width, s_width, c_width, f_width)) in DOMAIN_WIDTHS.iter().enumerate() {
            let max_rows = rows
                .iter()
                .filter_map(Option::as_ref)
                .map(|row| row.domains[domain].len())
                .max()
                .unwrap_or(0)
                .max(1);
            let mut exact_u = vec![0; batches * max_rows * u_width];
            let mut exact_s = vec![0; batches * max_rows * s_width];
            let mut semantic = vec![0; batches * max_rows * c_width];
            let mut numeric = vec![0.0; batches * max_rows * f_width];
            let mut scope = vec![0; batches * max_rows];
            let mut mask = vec![0; batches * max_rows];
            for (batch, row) in rows.iter().enumerate() {
                let Some(row) = row else { continue };
                for (position, record) in row.domains[domain].iter().enumerate() {
                    let base = batch * max_rows + position;
                    exact_u[base * u_width..(base + 1) * u_width].copy_from_slice(&record.u);
                    exact_s[base * s_width..(base + 1) * s_width].copy_from_slice(&record.s);
                    semantic[base * c_width..(base + 1) * c_width].copy_from_slice(&record.c);
                    numeric[base * f_width..(base + 1) * f_width].copy_from_slice(&record.f);
                    scope[base] = record.scope;
                    mask[base] = 1;
                }
            }
            packed_domains.push(
                PyTuple::new(
                    py,
                    [
                        ndarray::Array3::from_shape_vec((batches, max_rows, u_width), exact_u)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array3::from_shape_vec((batches, max_rows, s_width), exact_s)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array3::from_shape_vec((batches, max_rows, c_width), semantic)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array3::from_shape_vec((batches, max_rows, f_width), numeric)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array2::from_shape_vec((batches, max_rows), scope)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array2::from_shape_vec((batches, max_rows), mask)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                    ],
                )?
                .into_any(),
            );
        }
        let domains = PyTuple::new(py, packed_domains)?;
        let actions = PyTuple::new(
            py,
            [
                ndarray::Array3::from_shape_vec((batches, max_actions, ACTION_U), action_u)
                    .unwrap()
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array3::from_shape_vec((batches, max_actions, ACTION_S), action_s)
                    .unwrap()
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array3::from_shape_vec((batches, max_actions, ACTION_C), action_c)
                    .unwrap()
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array3::from_shape_vec((batches, max_actions, ACTION_F), action_f)
                    .unwrap()
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array2::from_shape_vec((batches, max_actions), represented)
                    .unwrap()
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array2::from_shape_vec((batches, max_actions), legal)
                    .unwrap()
                    .into_pyarray(py)
                    .into_any(),
            ],
        )?;
        for (batch, row) in rows.iter().enumerate() {
            if let Some(row) = row {
                digests[batch] = packed_observation(row).4;
            }
        }
        self.actions = rows
            .iter()
            .map(|row| {
                row.as_ref().map_or_else(Vec::new, |row| {
                    row.candidates
                        .iter()
                        .map(|candidate| candidate.action.clone())
                        .collect()
                })
            })
            .collect();
        PyTuple::new(
            py,
            [
                ndarray::Array1::from_vec(characters)
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array2::from_shape_vec((batches, globals_len(layout)), globals)
                    .unwrap()
                    .into_pyarray(py)
                    .into_any(),
                domains.into_any(),
                actions.into_any(),
                ndarray::Array1::from_vec(potentials)
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array1::from_vec(digests)
                    .into_pyarray(py)
                    .into_any(),
            ],
        )
    }

    fn load_policy(&mut self, data: &[u8]) -> PyResult<()> {
        self.policy = Some(
            ValueModel::from_bytes(data, &self.content)
                .map_err(|error| PyValueError::new_err(error.to_string()))?,
        );
        Ok(())
    }

    fn policy_details(
        &self,
        indices: Vec<usize>,
        temperature: f32,
    ) -> PyResult<Vec<(usize, Vec<f32>, f32)>> {
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(PyValueError::new_err("temperature must be positive"));
        }
        if indices.is_empty() {
            return Ok(Vec::new());
        }
        let model = self
            .policy
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("policy is not loaded"))?;
        let rows = indices
            .iter()
            .map(|&index| {
                self.games.get(index).map(|game| {
                    observation_v56(
                        game,
                        &self.content,
                        self.layout,
                        (self.training_strength, self.training_dexterity),
                    )
                })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
        let references = rows.iter().collect::<Vec<_>>();
        let features = model.state_actions_batch(&references);
        let outputs = model
            .evaluate_batch(&references, &features, temperature, None, true)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(indices
            .into_iter()
            .zip(outputs)
            .map(|(index, (log_policy, _, _, values, _))| (index, log_policy, values[0]))
            .collect())
    }

    #[pyo3(signature = (
            index=0,
            simulations=128,
            turns=1,
            max_depth=64,
            batch_size=256,
            prior_temperature=1.0,
            exploration=1.5,
            policy_temperature=0.8,
            exact_samples=16,
            max_exact_states=100_000,
            seed=1,
            progress=false,
            q_temperature=0.002,
        ))]
    fn search_diagnostics(
        &self,
        py: Python<'_>,
        index: usize,
        simulations: usize,
        turns: usize,
        max_depth: usize,
        batch_size: usize,
        prior_temperature: f32,
        exploration: f32,
        policy_temperature: f32,
        exact_samples: usize,
        max_exact_states: usize,
        seed: u64,
        progress: bool,
        q_temperature: f32,
    ) -> PyResult<String> {
        if simulations == 0
            || max_depth == 0
            || batch_size == 0
            || exact_samples == 0
            || max_exact_states == 0
            || !prior_temperature.is_finite()
            || prior_temperature <= 0.0
            || !exploration.is_finite()
            || exploration < 0.0
            || !policy_temperature.is_finite()
            || policy_temperature <= 0.0
            || !q_temperature.is_finite()
            || q_temperature <= 0.0
        {
            return Err(PyValueError::new_err("invalid search diagnostic settings"));
        }
        let game = self
            .games
            .get(index)
            .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
        if game.combat().is_none() {
            return Err(PyValueError::new_err("diagnostic root is not in combat"));
        }
        let model = self
            .policy
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("policy is not loaded"))?;
        let result = py.allow_threads(|| {
            search_diagnostic(
                model,
                game,
                &self.content,
                self.layout,
                (self.training_strength, self.training_dexterity),
                simulations,
                turns,
                max_depth,
                batch_size,
                prior_temperature,
                exploration,
                policy_temperature,
                exact_samples,
                max_exact_states,
                seed,
                progress,
                q_temperature,
            )
        });
        serde_json::to_string_pretty(&result.map_err(PyValueError::new_err)?)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    #[pyo3(signature = (
            indices,
            turns=1,
            max_depth=64,
            samples=16,
            max_states=100_000,
            seed=1,
            progress=true,
            heuristic=false,
        ))]
    fn exact_choices(
        &self,
        py: Python<'_>,
        indices: Vec<usize>,
        turns: usize,
        max_depth: usize,
        samples: usize,
        max_states: usize,
        seed: u64,
        progress: bool,
        heuristic: bool,
    ) -> PyResult<
        Vec<(
            i64,
            f32,
            u64,
            u64,
            u64,
            bool,
            Vec<(Vec<u8>, i64, u64)>,
            String,
        )>,
    > {
        if max_depth == 0 || samples == 0 || max_states == 0 {
            return Err(PyValueError::new_err("invalid exact-search settings"));
        }
        let model = self
            .policy
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("policy is not loaded"))?;
        let content = &self.content;
        let layout = self.layout;
        let bonuses = (self.training_strength, self.training_dexterity);
        Ok(py.allow_threads(|| {
            indices
                .par_iter()
                .map(|&index| {
                    let result = (|| {
                        let game = self
                            .games
                            .get(index)
                            .ok_or_else(|| "invalid environment index".to_owned())?;
                        let root_turn = game
                            .combat()
                            .ok_or_else(|| "exact-search root is not in combat".to_owned())?
                            .turn;
                        let observation = observation_v56(game, content, layout, bonuses);
                        let search_seed = seed
                            ^ game.seed as u64
                            ^ observation_digest(&observation).rotate_left(17);
                        let (result, search) = exact_search(
                            model,
                            game,
                            content,
                            layout,
                            bonuses,
                            root_turn,
                            turns,
                            max_depth,
                            samples,
                            max_states,
                            search_seed,
                            false,
                            progress,
                            heuristic,
                        )?;
                        let policy = observation
                            .candidates
                            .iter()
                            .map(|candidate| {
                                if candidate.legal {
                                    0.0
                                } else {
                                    f32::NEG_INFINITY
                                }
                            })
                            .collect::<Vec<_>>();
                        let node = SearchNode::new(
                            observation.candidates.clone(),
                            &policy,
                            0.0,
                            0.0,
                            0,
                            1.0,
                            None,
                        );
                        if search.action_values.len() != node.edges.len() {
                            return Err("exact search returned incomplete root actions".to_owned());
                        }
                        let edge = search
                            .action_values
                            .iter()
                            .enumerate()
                            .filter(|(_, value)| value.is_finite())
                            .max_by(|left, right| left.1.total_cmp(right.1))
                            .map(|(edge, _)| edge)
                            .ok_or_else(|| "exact-search root has no action".to_owned())?;
                        let mut choices = search
                            .choices
                            .into_iter()
                            .map(|((_digest, depth), (row, choice))| {
                                (row, choice as i64, depth as u64)
                            })
                            .collect::<Vec<_>>();
                        choices.sort_by_key(|(_, _, depth)| *depth);
                        Ok::<_, String>((
                            node.edges[edge].candidate as i64,
                            result.value,
                            search.states as u64,
                            search.transitions as u64,
                            search.rng_transitions as u64,
                            result.rng,
                            choices,
                        ))
                    })();
                    match result {
                        Ok((choice, value, states, transitions, rng_transitions, rng, plan)) => (
                            choice,
                            value,
                            states,
                            transitions,
                            rng_transitions,
                            rng,
                            plan,
                            String::new(),
                        ),
                        Err(error) => (-1, 0.0, 0, 0, 0, false, Vec::new(), error),
                    }
                })
                .collect()
        }))
    }

    #[pyo3(signature = (
            temperature=1.0,
            sample=true,
            advance=false,
            mcts_fraction=0.0,
            mcts_simulations=0,
            mcts_boss_simulations=0,
            mcts_turns=1,
            mcts_max_depth=64,
            mcts_batch_size=256,
            mcts_min_visits=16,
            mcts_max_targets=64,
            mcts_prior_temperature=1.0,
            mcts_q_temperature=0.002,
            mcts_exploration=1.5,
            mcts_value_consistency=false,
            mcts_heuristic=false,
            mcts_timeout=0.0,
            cache_features=false,
            skip_forced=false,
        ))]
    fn policy<'py>(
        &mut self,
        py: Python<'py>,
        temperature: f32,
        sample: bool,
        advance: bool,
        mcts_fraction: f32,
        mcts_simulations: usize,
        mcts_boss_simulations: usize,
        mcts_turns: usize,
        mcts_max_depth: usize,
        mcts_batch_size: usize,
        mcts_min_visits: u32,
        mcts_max_targets: usize,
        mcts_prior_temperature: f32,
        mcts_q_temperature: f32,
        mcts_exploration: f32,
        mcts_value_consistency: bool,
        mcts_heuristic: bool,
        mcts_timeout: f64,
        cache_features: bool,
        skip_forced: bool,
    ) -> PyResult<Bound<'py, PyTuple>> {
        if !(0.0..=1.0).contains(&mcts_fraction)
            || mcts_max_depth == 0
            || mcts_batch_size == 0
            || mcts_min_visits == 0
            || mcts_max_targets == 0
            || !mcts_prior_temperature.is_finite()
            || mcts_prior_temperature <= 0.0
            || !mcts_q_temperature.is_finite()
            || mcts_q_temperature <= 0.0
            || !mcts_exploration.is_finite()
            || mcts_exploration < 0.0
            || !mcts_timeout.is_finite()
            || mcts_timeout < 0.0
        {
            return Err(PyValueError::new_err("invalid MCTS settings"));
        }
        let search_enabled = mcts_simulations > 0 || mcts_boss_simulations > 0;
        let turn_starts = self
            .games
            .iter()
            .zip(&mut self.searched_turns)
            .map(|(game, searched)| {
                let Some(turn) = game.combat().map(|combat| combat.turn) else {
                    *searched = None;
                    return false;
                };
                let fresh = search_enabled && *searched != Some(turn);
                if search_enabled {
                    *searched = Some(turn);
                }
                fresh
            })
            .collect::<Vec<_>>();
        let model = self
            .policy
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("policy is not loaded"))?;
        let content = &self.content;
        let layout = self.layout;
        let bonuses = (self.training_strength, self.training_dexterity);
        let potential_weights = self.potential_weights;
        let skip_forced = (cache_features || skip_forced) && !search_enabled;
        let rows = py.allow_threads(|| {
            self.games
                .par_iter()
                .map(|game| {
                    let potential = potential(game, content);
                    let (mut actions, legal) = candidate_actions(game, content);
                    if !skip_forced || actions.len() != 1 {
                        return observation_v56_with_candidates(
                            game, content, layout, bonuses, None, actions, legal,
                        );
                    }
                    ObservationV56 {
                        character: game.run.character as u8,
                        globals: Vec::new(),
                        domains: std::array::from_fn(|_| DomainRows::Owned(Vec::new())),
                        candidates: vec![CandidateRow {
                            action: actions.pop().unwrap(),
                            u: [0; ACTION_U],
                            s: [0; ACTION_S],
                            c: [0; ACTION_C],
                            f: [0.0; ACTION_F],
                            legal: true,
                        }],
                        potential,
                    }
                })
                .collect::<Vec<_>>()
        });
        let evaluated_rows = rows
            .iter()
            .filter(|row| !skip_forced || row.candidates.len() > 1)
            .collect::<Vec<_>>();
        let features = py.allow_threads(|| model.state_actions_batch(&evaluated_rows));
        let packed = py.allow_threads(|| {
            rows.par_iter()
                .map(|row| {
                    if cache_features || skip_forced && row.candidates.len() == 1 {
                        cached_packed_metadata(row)
                    } else {
                        compact_packed_observation(row)
                    }
                })
                .collect::<Vec<_>>()
        });
        let outputs = model
            .evaluate_batch(&evaluated_rows, &features, temperature, None, true)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let mut random = self.random;
        let mut search_random = random ^ 0x4d43_5453_524e_4701;
        let (expert_targets, search_stats) = if search_enabled {
            let started = std::time::Instant::now();
            let (targets, mut stats) = py
                .allow_threads(|| {
                    mcts_targets(
                        model,
                        &self.games,
                        &rows,
                        &outputs,
                        &turn_starts,
                        content,
                        layout,
                        bonuses,
                        &mut search_random,
                        mcts_fraction,
                        mcts_simulations,
                        mcts_boss_simulations,
                        mcts_turns,
                        mcts_max_depth,
                        mcts_batch_size,
                        mcts_min_visits,
                        mcts_max_targets,
                        mcts_prior_temperature,
                        mcts_q_temperature,
                        mcts_exploration,
                        temperature,
                        mcts_value_consistency,
                        mcts_heuristic,
                        mcts_timeout,
                    )
                })
                .map_err(PyValueError::new_err)?;
            stats.micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            if stats.roots > 0 {
                tracing::debug!(
                    turn_starts = stats.turn_starts,
                    roots = stats.roots,
                    simulations = stats.simulations,
                    leaves = stats.leaves,
                    nodes = stats.nodes,
                    batches = stats.batches,
                    targets = stats.targets,
                    elapsed_us = stats.micros,
                    simulate_us = stats.simulate_micros,
                    encode_us = stats.encode_micros,
                    inference_us = stats.inference_micros,
                    backup_us = stats.backup_micros,
                    rollout_steps = stats.rollout_steps,
                    rollout_completed = stats.rollout_completed,
                    rollout_invalid = stats.rollout_invalid,
                    rollout_us = stats.rollout_micros,
                    timed_out = stats.timed_out,
                    "mcts"
                );
                if stats.timed_out {
                    tracing::warn!(
                        roots = stats.roots,
                        simulations = stats.simulations,
                        elapsed_us = stats.micros,
                        "mcts_timeout"
                    );
                } else if stats.micros > 5_000_000 {
                    tracing::warn!(
                        roots = stats.roots,
                        simulations = stats.simulations,
                        elapsed_us = stats.micros,
                        "slow_mcts"
                    );
                }
            }
            (targets, stats)
        } else {
            (Vec::new(), SearchStats::default())
        };
        let mut characters = Vec::with_capacity(rows.len());
        let mut choices = Vec::with_capacity(rows.len());
        let mut log_probabilities = Vec::with_capacity(rows.len());
        let mut critic_values = Vec::with_capacity(rows.len());
        let mut packed_rows = Vec::with_capacity(rows.len());
        let mut cached_features = Vec::with_capacity(rows.len());
        let mut selected_actions = Vec::with_capacity(rows.len());
        let potentials = rows
            .iter()
            .map(|row| potential_value(&row.potential, &potential_weights))
            .collect::<Vec<_>>();
        self.actions.clear();
        if advance {
            self.actions.resize_with(rows.len(), Vec::new);
        }
        let mut evaluated = features.into_iter().zip(outputs);
        for (row, packed) in rows.into_iter().zip(packed) {
            if skip_forced && row.candidates.len() == 1 {
                characters.push(row.character);
                choices.push(0);
                log_probabilities.push(0.0);
                critic_values.push(0.0);
                if advance {
                    selected_actions.push(row.candidates[0].action.clone());
                } else {
                    self.actions.push(vec![row.candidates[0].action.clone()]);
                }
                cached_features.push(PyBytes::new(py, &[]));
                packed_rows.push(PyBytes::new(py, &packed));
                continue;
            }
            let ((state, actions), (log_policy, _win, _expected, values, _)) =
                evaluated.next().unwrap();
            let choice = if sample {
                sample_policy(&log_policy, &mut random)
            } else {
                log_policy
                    .iter()
                    .enumerate()
                    .filter(|(_, value)| value.is_finite())
                    .max_by(|left, right| left.1.total_cmp(right.1))
                    .map(|(index, _)| index)
            }
            .ok_or_else(|| PyValueError::new_err("observation has no legal action"))?;
            characters.push(row.character);
            choices.push(choice as i64);
            log_probabilities.push(log_policy[choice]);
            critic_values.push(values[0]);
            if advance {
                selected_actions.push(row.candidates[choice].action.clone());
            } else {
                self.actions.push(
                    row.candidates
                        .iter()
                        .map(|candidate| candidate.action.clone())
                        .collect(),
                );
            }
            if cache_features {
                let mut bytes = Vec::with_capacity((actions.len() + 1) * state.len() * 4);
                for value in std::iter::once(&state).chain(&actions) {
                    #[cfg(target_endian = "little")]
                    bytes.extend_from_slice(unsafe {
                        std::slice::from_raw_parts(value.as_ptr().cast(), value.len() * 4)
                    });
                    #[cfg(target_endian = "big")]
                    value
                        .iter()
                        .for_each(|number| bytes.extend_from_slice(&number.to_le_bytes()));
                }
                cached_features.push(PyBytes::new(py, &bytes));
            }
            packed_rows.push(PyBytes::new(py, &packed));
        }
        debug_assert!(evaluated.next().is_none());
        self.random = random;
        let expert_targets = expert_targets
            .into_iter()
            .map(|target| {
                if target.consistency.self_weight == 1.0 {
                    return PyTuple::new(
                        py,
                        [
                            PyBytes::new(py, &target.packed).into_any(),
                            ndarray::Array1::from_vec(target.target)
                                .into_pyarray(py)
                                .into_any(),
                            target.visits.into_pyobject(py)?.into_any(),
                            target.depth.into_pyobject(py)?.into_any(),
                        ],
                    )
                    .map(Bound::into_any);
                }
                let children = PyTuple::new(
                    py,
                    target
                        .consistency
                        .packed
                        .iter()
                        .map(|packed| PyBytes::new(py, packed)),
                )?;
                PyTuple::new(
                    py,
                    [
                        PyBytes::new(py, &target.packed).into_any(),
                        ndarray::Array1::from_vec(target.target)
                            .into_pyarray(py)
                            .into_any(),
                        target.visits.into_pyobject(py)?.into_any(),
                        target.depth.into_pyobject(py)?.into_any(),
                        children.into_any(),
                        ndarray::Array1::from_vec(target.consistency.weights)
                            .into_pyarray(py)
                            .into_any(),
                        target.consistency.self_weight.into_pyobject(py)?.into_any(),
                        target
                            .consistency
                            .terminal_value
                            .into_pyobject(py)?
                            .into_any(),
                    ],
                )
                .map(Bound::into_any)
            })
            .collect::<PyResult<Vec<_>>>()?;
        let mut output = vec![
            ndarray::Array1::from_vec(characters)
                .into_pyarray(py)
                .into_any(),
            ndarray::Array1::from_vec(choices)
                .into_pyarray(py)
                .into_any(),
            ndarray::Array1::from_vec(log_probabilities)
                .into_pyarray(py)
                .into_any(),
            ndarray::Array1::from_vec(critic_values)
                .into_pyarray(py)
                .into_any(),
            PyTuple::new(py, packed_rows)?.into_any(),
            PyTuple::new(py, expert_targets)?.into_any(),
            vec![
                search_stats.roots as u64,
                search_stats.simulations as u64,
                search_stats.leaves as u64,
                search_stats.nodes as u64,
                search_stats.batches as u64,
                search_stats.targets as u64,
                search_stats.turn_starts as u64,
                search_stats.micros,
                search_stats.simulate_micros,
                search_stats.encode_micros,
                search_stats.inference_micros,
                search_stats.backup_micros,
                search_stats.rollout_steps as u64,
                search_stats.rollout_completed as u64,
                search_stats.rollout_invalid as u64,
                search_stats.rollout_micros,
                search_stats.timed_out as u64,
            ]
            .into_pyobject(py)?
            .into_any(),
        ];
        if advance {
            let canonical = self
                .games
                .iter()
                .map(canonical_progress)
                .collect::<Vec<_>>();
            let phases = self
                .games
                .iter()
                .map(|game| phase_index(&game.phase) as u8)
                .collect::<Vec<_>>();
            let paths = selected_actions
                .iter()
                .map(|action| matches!(action, Action::Path(_)))
                .collect::<Vec<_>>();
            let content = &self.content;
            let results = py
                .allow_threads(|| {
                    self.games
                        .par_iter_mut()
                        .zip(selected_actions)
                        .map(|(game, action)| {
                            let was_combat = game.combat().is_some();
                            game.step(content, action)
                                .map_err(|error| format!("{error:?}"))?;
                            Ok((
                                matches!(game.phase, Phase::Won) as u8 as f32,
                                matches!(game.phase, Phase::Won | Phase::Dead),
                                was_combat,
                                !was_combat && game.combat().is_some(),
                            ))
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
                .map_err(PyValueError::new_err)?;
            for plan in &mut self.plans {
                plan.clear();
            }
            for (index, path) in paths.into_iter().enumerate() {
                if path {
                    self.remember(index);
                }
            }
            for (game, result) in self.games.iter_mut().zip(&results) {
                if result.3 {
                    apply_training_bonuses(
                        game,
                        &self.content,
                        self.training_strength,
                        self.training_dexterity,
                    );
                }
            }
            let rewards = results.iter().map(|result| result.0).collect::<Vec<_>>();
            let done = results.iter().map(|result| result.1).collect::<Vec<_>>();
            let in_combat = results.iter().map(|result| result.2).collect::<Vec<_>>();
            let stats = self.stats();
            let legal = self
                .games
                .par_iter()
                .zip(&done)
                .map(|(game, done)| {
                    !done
                        && candidate_actions(game, &self.content)
                            .1
                            .into_iter()
                            .any(|legal| legal)
                })
                .collect::<Vec<_>>();
            output.extend([
                ndarray::Array1::from_vec(rewards)
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array1::from_vec(done).into_pyarray(py).into_any(),
                stats.into_pyobject(py)?.into_any(),
                ndarray::Array1::from_vec(legal).into_pyarray(py).into_any(),
                ndarray::Array1::from_vec(in_combat)
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array1::from_vec(canonical)
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array1::from_vec(phases)
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array1::from_vec(potentials)
                    .into_pyarray(py)
                    .into_any(),
            ]);
        }
        if cache_features {
            output.push(PyTuple::new(py, cached_features)?.into_any());
        }
        PyTuple::new(py, output)
    }

    #[pyo3(signature = (active=None))]
    fn has_legal_actions(&self, active: Option<Vec<bool>>) -> PyResult<Vec<bool>> {
        let active = active.unwrap_or_else(|| vec![true; self.games.len()]);
        if active.len() != self.games.len() {
            return Err(PyValueError::new_err("invalid active mask"));
        }
        Ok(self
            .games
            .par_iter()
            .zip(active)
            .map(|(game, active)| {
                active
                    && candidate_actions(game, &self.content)
                        .1
                        .into_iter()
                        .any(|legal| legal)
            })
            .collect())
    }

    #[pyo3(signature = (choices, active=None))]
    fn step(
        &mut self,
        py: Python<'_>,
        choices: Vec<usize>,
        active: Option<Vec<bool>>,
    ) -> PyResult<(Vec<f32>, Vec<bool>, Vec<f32>)> {
        if choices.len() != self.games.len() || self.actions.len() != self.games.len() {
            return Err(PyValueError::new_err("observe before step"));
        }
        let active = active.unwrap_or_else(|| vec![true; self.games.len()]);
        if active.len() != self.games.len() {
            return Err(PyValueError::new_err("invalid active mask"));
        }
        self.actions
            .iter()
            .zip(&choices)
            .zip(&active)
            .enumerate()
            .try_for_each(|(environment, ((actions, &choice), &active))| {
                if active && choice >= actions.len() {
                    return Err(format!(
                        "environment {environment} chose action {choice} from {}",
                        actions.len()
                    ));
                }
                Ok(())
            })
            .map_err(PyValueError::new_err)?;
        let selected = self
            .actions
            .iter_mut()
            .zip(choices)
            .zip(&active)
            .map(|((actions, choice), &active)| active.then(|| actions.swap_remove(choice)))
            .collect::<Vec<_>>();
        let paths = selected
            .iter()
            .map(|action| matches!(action, Some(Action::Path(_))))
            .collect::<Vec<_>>();
        let content = &self.content;
        let potential_weights = self.potential_weights;
        let results = py
            .allow_threads(|| {
                self.games
                    .par_iter_mut()
                    .zip(selected.into_par_iter())
                    .map(|(game, action)| {
                        let Some(action) = action else {
                            return Ok((0.0, false, 0.0, false));
                        };
                        let was_combat = game.combat().is_some();
                        game.step(content, action)
                            .map_err(|error| format!("{error:?}"))?;
                        let reward = matches!(game.phase, Phase::Won) as u8 as f32;
                        let done = matches!(game.phase, Phase::Won | Phase::Dead);
                        Ok((
                            reward,
                            done,
                            potential_value(&potential(game, content), &potential_weights),
                            !was_combat && game.combat().is_some(),
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .map_err(PyValueError::new_err)?;
        for plan in &mut self.plans {
            plan.clear();
        }
        for (index, path) in paths.into_iter().enumerate() {
            if path {
                self.remember(index);
            }
        }
        for (game, result) in self.games.iter_mut().zip(&results) {
            if result.3 {
                apply_training_bonuses(
                    game,
                    &self.content,
                    self.training_strength,
                    self.training_dexterity,
                );
            }
        }
        let mut rewards = Vec::with_capacity(results.len());
        let mut done = Vec::with_capacity(results.len());
        let mut potentials = Vec::with_capacity(results.len());
        for (reward, terminal, value, _) in results {
            rewards.push(reward);
            done.push(terminal);
            potentials.push(value);
        }
        Ok((rewards, done, potentials))
    }

    #[pyo3(signature = (indices, archive_probability=0.0))]
    fn reset(&mut self, indices: Vec<usize>, archive_probability: f32) -> PyResult<()> {
        for index in indices {
            if index >= self.games.len() {
                return Err(PyValueError::new_err("invalid environment index"));
            }
            let archived = self.random_f32() < archive_probability.clamp(0.0, 1.0);
            (self.games[index], self.root_ids[index]) = if archived {
                if let Some(root) = self.archived_game() {
                    root
                } else {
                    (self.fresh_game()?, usize::MAX)
                }
            } else {
                (self.fresh_game()?, usize::MAX)
            };
            apply_training_bonuses(
                &mut self.games[index],
                &self.content,
                self.training_strength,
                self.training_dexterity,
            );
            self.actions[index].clear();
            self.plans[index].clear();
            self.searched_turns[index] = None;
        }
        Ok(())
    }

    fn rust_values(&self, path: &str) -> PyResult<Vec<f32>> {
        let model = ValueModel::load(path, &self.content)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(self
            .games
            .par_iter()
            .map(|game| model.win_probability(game, &self.content))
            .collect())
    }
}

impl Batch {
    fn random_u64(&mut self) -> u64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.random
    }

    fn random_f32(&mut self) -> f32 {
        (self.random_u64() >> 40) as f32 / (1u32 << 24) as f32
    }

    fn fresh_game(&mut self) -> PyResult<Game> {
        let character = self
            .character
            .unwrap_or_else(|| (self.random_u64() as usize % self.content.characters.len()) as Id);
        let seed = self.next_seed;
        self.next_seed = self.next_seed.wrapping_add(self.seed_stride);
        let mut game =
            Game::new_character_ascension(&self.content, seed, character, self.ascension)
                .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
        game.begin_run(&self.content)
            .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
        Ok(game)
    }

    fn remember(&mut self, index: usize) {
        let game = self.games[index].clone();
        if matches!(game.phase, Phase::Won | Phase::Dead) {
            return;
        }
        if index == 0 {
            self.first_archive.push(game.clone());
        }
        let floor =
            (game.run.act.saturating_sub(1) as usize * 18 + game.run.floor as usize).min(63);
        let key = (game.run.character as usize * 64 + floor) * PHASES + phase_index(&game.phase);
        self.archive_seen[key] += 1;
        let replacement = self.random_u64() % self.archive_seen[key];
        let bucket = &mut self.archive[key];
        if bucket.len() < 16 {
            bucket.push(game);
        } else if replacement < 16 {
            bucket[replacement as usize] = game;
        }
    }

    fn archived_game(&mut self) -> Option<(Game, usize)> {
        let character = if let Some(character) = self.character {
            character as usize
        } else {
            let characters = (0..self.content.characters.len())
                .filter(|character| {
                    self.archive[character * 64 * PHASES..(character + 1) * 64 * PHASES]
                        .iter()
                        .any(|bucket| !bucket.is_empty())
                })
                .collect::<Vec<_>>();
            if characters.is_empty() {
                return None;
            }
            *characters.get(self.random_u64() as usize % characters.len())?
        };
        let highest = self
            .archive
            .iter()
            .enumerate()
            .filter(|(key, bucket)| !bucket.is_empty() && *key / (64 * PHASES) == character)
            .map(|(key, _)| key / PHASES % 64)
            .max()?;
        let target = highest.saturating_sub(self.archive_depth);
        let distance = self
            .archive
            .iter()
            .enumerate()
            .filter(|(key, bucket)| !bucket.is_empty() && key / (64 * PHASES) == character)
            .map(|(key, _)| (key / PHASES % 64).abs_diff(target))
            .min()?;
        let candidates = self
            .archive
            .iter()
            .enumerate()
            .filter(|(key, bucket)| {
                !bucket.is_empty()
                    && (key / PHASES % 64).abs_diff(target) == distance
                    && key / (64 * PHASES) == character
            })
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        let key = candidates[self.random_u64() as usize % candidates.len()];
        let index = self.random_u64() as usize % self.archive[key].len();
        let mut game = self.archive[key][index].clone();
        if self.resample_archive {
            let seed = self.random_u64();
            resample_hidden(&mut game, &self.content, seed);
        }
        Some((game, key * 16 + index))
    }
}

#[pymodule]
fn sts2_sim(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Batch>()?;
    module.add_function(wrap_pyfunction!(model_schema, module)?)?;
    module.add_function(wrap_pyfunction!(configure_logging, module)?)?;
    module.add_function(wrap_pyfunction!(unique_rows, module)?)?;
    module.add_function(wrap_pyfunction!(unique_feature_rows, module)?)?;
    module.add_function(wrap_pyfunction!(unique_graphs, module)?)?;
    module.add_function(wrap_pyfunction!(validate_action_features, module)?)?;
    module.add_function(wrap_pyfunction!(compress_packed_observations, module)?)?;
    module.add_function(wrap_pyfunction!(validate_packed_observation, module)?)?;
    module.add_function(wrap_pyfunction!(validate_compact_observation, module)?)?;
    module.add_function(wrap_pyfunction!(unpack_packed_observations, module)?)
}

#[test]
fn rescue_ends_turn_after_32_manual_plays() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let actions = game.actions(&content);
    let play = actions
        .iter()
        .position(|action| matches!(action, Action::Play { .. }))
        .unwrap();
    let end_turn = actions
        .iter()
        .position(|action| matches!(action, Action::EndTurn))
        .unwrap();
    let Phase::Combat(combat) = &mut game.phase else {
        panic!("combat did not start");
    };
    combat.history.manual_plays = 31;
    assert_eq!(rescue_choice(&game, &content, play, 0.5, true), play);
    let Phase::Combat(combat) = &mut game.phase else {
        panic!("combat ended");
    };
    combat.history.manual_plays = 32;
    assert_eq!(rescue_choice(&game, &content, play, 0.5, true), end_turn);
}

#[test]
fn run_search_returns_live_partial_combat() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let Phase::Combat(combat) = &mut game.phase else {
        panic!("combat did not start")
    };
    combat.player.hp = 30_000;
    combat.player.max_hp = 30_000;
    combat.enemies.truncate(1);
    combat.enemies[0].creature.hp = 30_000;
    combat.enemies[0].creature.max_hp = 30_000;
    let (partial, plan) = run_search(&game, &content, 1, 1, 1, (0, 0));
    assert!(partial.combat().is_some());
    assert!(!plan.is_empty());
}

#[test]
fn training_bonuses_replace_strength_and_dexterity_exactly() {
    let mut batch = Batch::new(1, 1, Some(0), 32, 1, 10, 1).unwrap();
    batch.games[0]
        .start_combat(&batch.content, batch.content.acts[0].encounters[0])
        .unwrap();
    let amounts = |batch: &Batch| {
        ["POWER.STRENGTH_POWER", "POWER.DEXTERITY_POWER"].map(|id| {
            batch.games[0]
                .combat()
                .unwrap()
                .player
                .powers
                .iter()
                .find(|power| batch.content.powers[power.id as usize].id == id)
                .map_or(0, |power| power.amount)
        })
    };
    batch.set_training_bonuses(0, 0);
    assert_eq!(amounts(&batch), [0, 0]);
    batch.set_training_bonus(7);
    assert_eq!(amounts(&batch), [7, 3]);
    batch.set_training_bonuses(7, 4);
    assert_eq!(amounts(&batch), [7, 4]);
    batch.set_training_bonuses(0, 0);
    assert_eq!(amounts(&batch), [0, 0]);
}

#[test]
fn reset_invalidates_cached_actions() {
    let mut batch = Batch::new(1, 1, Some(0), 32, 1, 10, 1).unwrap();
    batch.actions[0] = batch.games[0].actions(&batch.content);
    assert!(!batch.actions[0].is_empty());
    batch.reset(vec![0], 0.0).unwrap();
    assert!(batch.actions[0].is_empty());
}

#[test]
fn repaired_path_replays_with_aligned_choices() {
    let mut batch = Batch::new(1, 1, Some(0), 32, 1, 10, 1).unwrap();
    batch.set_training_bonuses(7, 4);
    let mut source = batch.games[0].clone();
    let opening = source
        .actions(&batch.content)
        .iter()
        .position(|action| matches!(action, Action::Event(_)))
        .unwrap();
    source
        .step(
            &batch.content,
            source.actions(&batch.content)[opening].clone(),
        )
        .unwrap();
    let first = source
        .actions(&batch.content)
        .iter()
        .position(|action| matches!(action, Action::Path(_)))
        .unwrap();
    let first_action = source.actions(&batch.content)[first].clone();
    source.step(&batch.content, first_action.clone()).unwrap();
    apply_training_bonuses(&mut source, &batch.content, 20, 10);
    let (_, source_plan) = combat_search(&source, &batch.content, 16, 64);
    let mut choices = vec![opening, first];
    for action in source_plan {
        let choice = source
            .actions(&batch.content)
            .iter()
            .position(|legal| *legal == action)
            .unwrap();
        choices.push(choice);
        source.step(&batch.content, action).unwrap();
    }

    let (won, repaired, score) = batch
        .repair_path_at(0, 20, choices, 1, 16, 64, None)
        .unwrap();
    let mut replay = batch.games[0].clone();
    for (position, choice) in repaired.into_iter().enumerate() {
        let action = replay.actions(&batch.content).get(choice).cloned().unwrap();
        if position == 1 {
            assert_eq!(action, first_action);
        }
        let combat = replay.combat().is_some();
        replay.step(&batch.content, action).unwrap();
        if !combat && replay.combat().is_some() {
            apply_training_bonuses(
                &mut replay,
                &batch.content,
                batch.training_strength,
                batch.training_dexterity,
            );
        }
    }
    assert_eq!(won, matches!(replay.phase, Phase::Won));
    assert_eq!(score, teacher_state_score(&replay, &batch.content));
}

#[test]
fn combat_key_keeps_boss_history() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    game.room = Room::Boss;
    let boss = combat_key(&game, &content);
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.history.manual_plays = 1;
    assert_ne!(boss, combat_key(&game, &content));
    game.room = Room::Combat;
    let room = combat_key(&game, &content);
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.history.manual_plays = 0;
    assert_eq!(room, combat_key(&game, &content));
}
