use super::*;

pub(super) fn content_fingerprint(content: &Content) -> u64 {
    let mut hash = ContentHasher(0xcbf29ce484222325);
    content.hash(&mut hash);
    hash.finish()
}

struct TransformerLayer {
    qkv_w: Vec<f32>,
    qkv_b: Vec<f32>,
    out_w: Vec<f32>,
    out_b: Vec<f32>,
    norm1_w: Vec<f32>,
    norm1_b: Vec<f32>,
    norm2_w: Vec<f32>,
    norm2_b: Vec<f32>,
    linear1_w: Vec<f32>,
    linear1_b: Vec<f32>,
    linear2_w: Vec<f32>,
    linear2_b: Vec<f32>,
}

fn read_transformer_layers(
    input: &mut &[u8],
    count: usize,
    width: usize,
    feedforward: usize,
) -> io::Result<Vec<TransformerLayer>> {
    (0..count)
        .map(|_| {
            Ok(TransformerLayer {
                qkv_w: read_f32s(input, 3 * width * width)?,
                qkv_b: read_f32s(input, 3 * width)?,
                out_w: read_f32s(input, width * width)?,
                out_b: read_f32s(input, width)?,
                norm1_w: read_f32s(input, width)?,
                norm1_b: read_f32s(input, width)?,
                norm2_w: read_f32s(input, width)?,
                norm2_b: read_f32s(input, width)?,
                linear1_w: read_f32s(input, feedforward * width)?,
                linear1_b: read_f32s(input, feedforward)?,
                linear2_w: read_f32s(input, width * feedforward)?,
                linear2_b: read_f32s(input, width)?,
            })
        })
        .collect()
}

pub(super) struct LinearWeights {
    pub(super) w: Vec<f32>,
    pub(super) b: Vec<f32>,
}

impl LinearWeights {
    fn read(input: &mut &[u8], input_width: usize, output_width: usize) -> io::Result<Self> {
        Ok(Self {
            w: read_f32s(input, input_width * output_width)?,
            b: read_f32s(input, output_width)?,
        })
    }

    fn read_without_bias(
        input: &mut &[u8],
        input_width: usize,
        output_width: usize,
    ) -> io::Result<Self> {
        Ok(Self {
            w: read_f32s(input, input_width * output_width)?,
            b: vec![0.0; output_width],
        })
    }

    fn apply(&self, input: &[f32]) -> Vec<f32> {
        linear(input, &self.w, &self.b)
    }
}

pub(super) struct TokenEncoderWeights {
    pub(super) numeric: LinearWeights,
    pub(super) norm_w: Vec<f32>,
    pub(super) norm_b: Vec<f32>,
}

impl TokenEncoderWeights {
    fn read(input: &mut &[u8], _semantic: usize, numeric: usize, width: usize) -> io::Result<Self> {
        Ok(Self {
            numeric: LinearWeights::read_without_bias(input, numeric, width)?,
            norm_w: read_f32s(input, width)?,
            norm_b: read_f32s(input, width)?,
        })
    }

    pub(super) fn encode(
        &self,
        semantic: &[u32],
        numeric: &[f32],
        table: &[f32],
        width: usize,
    ) -> Vec<f32> {
        self.finish(semantic, table, width, self.numeric.apply(numeric))
    }

    fn finish(&self, semantic: &[u32], table: &[f32], width: usize, mut out: Vec<f32>) -> Vec<f32> {
        for &code in semantic.iter().filter(|&&code| code != 0) {
            let embedding = &table[code as usize * width..][..width];
            out.iter_mut()
                .zip(embedding)
                .for_each(|(value, embedding)| *value += embedding);
        }
        layer_norm(&mut out, &self.norm_w, &self.norm_b);
        out
    }
}

struct MapEncoding {
    nodes: std::collections::BTreeMap<u32, Vec<f32>>,
    key_values: Vec<Vec<f32>>,
    current: Mutex<HashMap<u32, Vec<f32>>>,
}

#[derive(Default)]
struct EncodingCache {
    rows: HashMap<(u64, u64), Vec<f32>>,
    maps: HashMap<Vec<u8>, Arc<MapEncoding>>,
}

struct GruWeights {
    input: LinearWeights,
    hidden: LinearWeights,
}

impl GruWeights {
    fn read(input: &mut &[u8], width: usize) -> io::Result<Self> {
        Ok(Self {
            input: LinearWeights::read(input, width, 3 * width)?,
            hidden: LinearWeights::read(input, width, 3 * width)?,
        })
    }

    fn apply(&self, sequence: impl IntoIterator<Item = Vec<f32>>, width: usize) -> Vec<f32> {
        let mut state = vec![0.0; width];
        for value in sequence {
            let input = self.input.apply(&value);
            let hidden = self.hidden.apply(&state);
            for column in 0..width {
                let reset = sigmoid(input[column] + hidden[column]);
                let update = sigmoid(input[width + column] + hidden[width + column]);
                let candidate =
                    (input[2 * width + column] + reset * hidden[2 * width + column]).tanh();
                state[column] = (1.0 - update) * candidate + update * state[column];
            }
        }
        state
    }
}

pub struct ValueModel {
    layout: Layout,
    width: usize,
    heads: usize,
    pooling: [u8; 13],
    semantic_embedding: Vec<f32>,
    encoders: Vec<TokenEncoderWeights>,
    action_encoder: TokenEncoderWeights,
    summary_seed: Vec<Vec<f32>>,
    pool_layers: Vec<Option<TransformerLayer>>,
    move_gru: GruWeights,
    continuation_gru: Option<GruWeights>,
    actor_norm_w: Vec<f32>,
    actor_norm_b: Vec<f32>,
    action_norm_w: Vec<f32>,
    action_norm_b: Vec<f32>,
    global_layers: Vec<TransformerLayer>,
    global_norm_w: Vec<f32>,
    global_norm_b: Vec<f32>,
    graph_norm_w: Vec<f32>,
    graph_norm_b: Vec<f32>,
    graph_query: LinearWeights,
    graph_key_value: LinearWeights,
    graph_edge: LinearWeights,
    graph_out: LinearWeights,
    graph_degree: LinearWeights,
    graph_ff_norm_w: Vec<f32>,
    graph_ff_norm_b: Vec<f32>,
    graph_ff1: LinearWeights,
    graph_ff2: LinearWeights,
    policy: Option<LinearWeights>,
    critic: LinearWeights,
    temperature: f32,
    bias: f32,
    potential_weights: [f32; POTENTIAL_WEIGHT_COUNT],
    encode_caches: Vec<Mutex<EncodingCache>>,
}

impl ValueModel {
    pub fn load(path: impl AsRef<Path>, content: &Content) -> io::Result<Self> {
        Self::from_bytes(&fs::read(path)?, content)
    }

    fn from_bytes(bytes: &[u8], content: &Content) -> io::Result<Self> {
        let mut input = bytes;
        let magic = take_bytes(&mut input, 8)?;
        let model_version = read_u32(&mut input)?;
        if magic != MAGIC
            || !(MIN_VALUE_MODEL_VERSION..=VALUE_MODEL_VERSION).contains(&model_version)
            || read_u32(&mut input)? != VERSION
            || read_u64(&mut input)? != content_fingerprint(content)
        {
            return Err(invalid("incompatible value model"));
        }
        let dimensions = (0..10)
            .map(|_| read_u32(&mut input))
            .collect::<io::Result<Vec<_>>>()?;
        let [
            width,
            layers,
            heads,
            feedforward,
            domains,
            concepts,
            concept_vocab,
            action_c,
            action_f,
            critic_outputs,
        ] = <[u32; 10]>::try_from(dimensions).unwrap();
        let widths = (0..domains)
            .map(|_| {
                Ok((
                    read_u32(&mut input)? as usize,
                    read_u32(&mut input)? as usize,
                    read_u32(&mut input)? as usize,
                    read_u32(&mut input)? as usize,
                ))
            })
            .collect::<io::Result<Vec<_>>>()?;
        let concept_sizes = (0..concepts)
            .map(|_| read_u32(&mut input))
            .collect::<io::Result<Vec<_>>>()?;
        let position_caps = (0..POSITION_CAPS.len())
            .map(|_| read_u32(&mut input))
            .collect::<io::Result<Vec<_>>>()?;
        let pooling = <[u8; 13]>::try_from(take_bytes(&mut input, 13)?).unwrap();
        let actor = take_bytes(&mut input, 1)?[0] != 0;
        let layout = Layout::new(content);
        let (width, layers, heads, feedforward) = (
            width as usize,
            layers as usize,
            heads as usize,
            feedforward as usize,
        );
        if !valid_model_shape(width, layers, heads, feedforward)
            || domains as usize != DOMAIN_NAMES.len()
            || concepts as usize != Semantic::Count as usize
            || concept_vocab as usize != layout.concept_vocab()
            || action_c as usize != ACTION_C
            || action_f as usize != ACTION_F
            || critic_outputs != 1
            || widths != DOMAIN_WIDTHS
            || concept_sizes != layout.semantic_sizes
            || position_caps != POSITION_CAPS
            || !valid_pooling(pooling)
        {
            return Err(invalid("value model shape mismatch"));
        }
        let temperature = read_f32(&mut input)?;
        let bias = read_f32(&mut input)?;
        let potential_weights = <[f32; POTENTIAL_WEIGHT_COUNT]>::try_from(read_f32s(
            &mut input,
            POTENTIAL_WEIGHT_COUNT,
        )?)
        .unwrap();
        if !temperature.is_finite()
            || temperature <= 0.0
            || !bias.is_finite()
            || potential_weights.iter().any(|weight| !weight.is_finite())
        {
            return Err(invalid("invalid value calibration"));
        }
        let semantic_embedding = read_f32s(&mut input, concept_vocab as usize * width)?;
        let encoders = DOMAIN_WIDTHS
            .iter()
            .map(|&(_, _, semantic, numeric)| {
                TokenEncoderWeights::read(&mut input, semantic, numeric, width)
            })
            .collect::<io::Result<Vec<_>>>()?;
        let action_encoder = TokenEncoderWeights::read(&mut input, ACTION_C, ACTION_F, width)?;
        let summary_seed = (0..17)
            .map(|_| read_f32s(&mut input, width))
            .collect::<io::Result<Vec<_>>>()?;
        let mut pool_layers = Vec::with_capacity(13);
        for (index, &mode) in pooling.iter().enumerate() {
            pool_layers.push(if transformer_pool(index, mode) {
                Some(read_transformer_layers(&mut input, 1, width, feedforward)?.remove(0))
            } else {
                None
            });
        }
        let move_gru = GruWeights::read(&mut input, width)?;
        let continuation_gru = (pooling[10] == 2)
            .then(|| GruWeights::read(&mut input, width))
            .transpose()?;
        let actor_norm_w = read_f32s(&mut input, width)?;
        let actor_norm_b = read_f32s(&mut input, width)?;
        let action_norm_w = read_f32s(&mut input, width)?;
        let action_norm_b = read_f32s(&mut input, width)?;
        let global_layers = read_transformer_layers(&mut input, layers, width, feedforward)?;
        let global_norm_w = read_f32s(&mut input, width)?;
        let global_norm_b = read_f32s(&mut input, width)?;
        let graph_norm_w = read_f32s(&mut input, width)?;
        let graph_norm_b = read_f32s(&mut input, width)?;
        let graph_query = LinearWeights::read(&mut input, width, width)?;
        let graph_key_value = LinearWeights::read(&mut input, width, 2 * width)?;
        let graph_edge = LinearWeights::read_without_bias(&mut input, width, 2 * width)?;
        let graph_out = LinearWeights::read(&mut input, width, width)?;
        let graph_degree = LinearWeights::read_without_bias(&mut input, 2, width)?;
        let graph_ff_norm_w = read_f32s(&mut input, width)?;
        let graph_ff_norm_b = read_f32s(&mut input, width)?;
        let graph_ff1 = LinearWeights::read(&mut input, width, feedforward)?;
        let graph_ff2 = LinearWeights::read(&mut input, feedforward, width)?;
        let policy = actor
            .then(|| LinearWeights::read(&mut input, width, 1))
            .transpose()?;
        let critic = LinearWeights::read(&mut input, width, 1)?;
        if !input.is_empty() {
            return Err(invalid("trailing value model data"));
        }
        Ok(Self {
            layout,
            width,
            heads,
            pooling,
            semantic_embedding,
            encoders,
            action_encoder,
            summary_seed,
            pool_layers,
            move_gru,
            continuation_gru,
            actor_norm_w,
            actor_norm_b,
            action_norm_w,
            action_norm_b,
            global_layers,
            global_norm_w,
            global_norm_b,
            graph_norm_w,
            graph_norm_b,
            graph_query,
            graph_key_value,
            graph_edge,
            graph_out,
            graph_degree,
            graph_ff_norm_w,
            graph_ff_norm_b,
            graph_ff1,
            graph_ff2,
            policy,
            critic,
            temperature,
            bias,
            potential_weights,
            encode_caches: (0..16).map(|_| Mutex::default()).collect(),
        })
    }

    fn embedding(&self, semantic: Semantic, value: u32) -> &[f32] {
        let code = self.layout.semantic(semantic, value) as usize;
        &self.semantic_embedding[code * self.width..][..self.width]
    }

    #[cfg(test)]
    pub(super) fn candidate_object_key(observation: &ObservationV56, index: usize) -> (u32, u32) {
        let candidate = &observation.candidates[index];
        (candidate.u[0], candidate.u[14])
    }

    fn encode_values(
        &self,
        cache: &mut EncodingCache,
        domain: usize,
        semantic: &[u32],
        numeric: &[f32],
        encoder: &TokenEncoderWeights,
    ) -> Vec<f32> {
        let mut key = (0xcbf2_9ce4_8422_2325, 0x9e37_79b9_7f4a_7c15);
        for value in std::iter::once(domain as u32)
            .chain(semantic.iter().copied())
            .chain(numeric.iter().map(|value| value.to_bits()))
        {
            key.0 = (key.0 ^ value as u64).wrapping_mul(0x100_0000_01b3);
            key.1 = (key.1 ^ value as u64).wrapping_mul(0x9e37_79b1_85eb_ca87);
        }
        if let Some(value) = cache.rows.get(&key) {
            return value.clone();
        }
        let value = encoder.encode(semantic, numeric, &self.semantic_embedding, self.width);
        cache.rows.insert(key, value.clone());
        value
    }

    fn encode(&self, cache: &mut EncodingCache, domain: usize, row: &DomainRow) -> Vec<f32> {
        self.encode_values(cache, domain, &row.c, &row.f, &self.encoders[domain])
    }

    fn tag(&self, mut value: Vec<f32>, role: u32, collection: Option<u32>) -> Vec<f32> {
        for (sum, embedding) in value
            .iter_mut()
            .zip(self.embedding(Semantic::TokenRole, role))
        {
            *sum += embedding;
        }
        if let Some(collection) = collection {
            for (sum, embedding) in value
                .iter_mut()
                .zip(self.embedding(Semantic::Collection, collection))
            {
                *sum += embedding;
            }
        }
        value
    }

    fn attention(&self, query: &[f32], keys_values: &[Vec<f32>], offset: usize) -> Vec<f32> {
        let dimension = self.width / self.heads;
        let mut output = vec![0.0; self.width];
        if keys_values.is_empty() {
            return output;
        }
        let mut weights = vec![0.0; keys_values.len()];
        for head in 0..self.heads {
            let columns = head * dimension..(head + 1) * dimension;
            for (weight, row) in weights.iter_mut().zip(keys_values) {
                *weight = query[columns.clone()]
                    .iter()
                    .zip(&row[offset..][columns.clone()])
                    .map(|(left, right)| left * right)
                    .sum::<f32>()
                    / (dimension as f32).sqrt();
            }
            let peak = weights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            weights
                .iter_mut()
                .for_each(|score| *score = (*score - peak).exp());
            let total = weights.iter().sum::<f32>();
            for (row, weight) in keys_values.iter().zip(weights.iter().copied()) {
                for column in columns.clone() {
                    output[column] += weight / total * row[offset + self.width + column];
                }
            }
        }
        output
    }

    fn attention_batch(
        &self,
        qkv: &[f32],
        selected: impl Iterator<Item = usize> + Clone,
    ) -> Vec<f32> {
        let rows = qkv.len() / (3 * self.width);
        let dimension = self.width / self.heads;
        let selected_len = selected.clone().count();
        let mut output = vec![0.0; selected_len * self.width];
        #[cfg(target_os = "macos")]
        {
            #[link(name = "Accelerate", kind = "framework")]
            unsafe extern "C" {
                fn cblas_sgemm(
                    order: i32,
                    transpose_a: i32,
                    transpose_b: i32,
                    rows: i32,
                    columns: i32,
                    inner: i32,
                    alpha: f32,
                    left: *const f32,
                    left_stride: i32,
                    right: *const f32,
                    right_stride: i32,
                    beta: f32,
                    output: *mut f32,
                    output_stride: i32,
                );
            }
            let dense = selected_len == rows && selected.clone().eq(0..rows);
            let mut query = vec![0.0; selected_len * dimension];
            let mut scores = vec![0.0; selected_len * rows];
            for head in 0..self.heads {
                let column = head * dimension;
                let (query, stride) = if dense {
                    (&qkv[column..], 3 * self.width)
                } else {
                    for (target, source) in query.chunks_exact_mut(dimension).zip(selected.clone())
                    {
                        target
                            .copy_from_slice(&qkv[source * 3 * self.width + column..][..dimension]);
                    }
                    (&query[..], dimension)
                };
                unsafe {
                    cblas_sgemm(
                        101,
                        111,
                        112,
                        selected_len as i32,
                        rows as i32,
                        dimension as i32,
                        (dimension as f32).sqrt().recip(),
                        query.as_ptr(),
                        stride as i32,
                        qkv[self.width + column..].as_ptr(),
                        (3 * self.width) as i32,
                        0.0,
                        scores.as_mut_ptr(),
                        rows as i32,
                    );
                }
                for score in scores.chunks_exact_mut(rows) {
                    let peak = score.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                    score.iter_mut().for_each(|value| *value -= peak);
                }
                exp_in_place(&mut scores);
                for score in scores.chunks_exact_mut(rows) {
                    let total = score.iter().sum::<f32>();
                    score.iter_mut().for_each(|value| *value /= total);
                }
                unsafe {
                    cblas_sgemm(
                        101,
                        111,
                        111,
                        selected_len as i32,
                        dimension as i32,
                        rows as i32,
                        1.0,
                        scores.as_ptr(),
                        rows as i32,
                        qkv[2 * self.width + column..].as_ptr(),
                        (3 * self.width) as i32,
                        0.0,
                        output[column..].as_mut_ptr(),
                        self.width as i32,
                    );
                }
            }
            output
        }
        #[cfg(not(target_os = "macos"))]
        {
            let qkv = qkv
                .chunks_exact(3 * self.width)
                .map(<[f32]>::to_vec)
                .collect::<Vec<_>>();
            for (target, source) in output.chunks_exact_mut(self.width).zip(selected) {
                target.copy_from_slice(&self.attention(
                    &qkv[source][..self.width],
                    &qkv,
                    self.width,
                ));
            }
            output
        }
    }

    fn transformed(
        &self,
        sequence: &[Vec<f32>],
        layer: &TransformerLayer,
        selected: impl Iterator<Item = usize> + Clone,
    ) -> Vec<Vec<f32>> {
        let normalized_rows = sequence
            .iter()
            .flat_map(|row| normalized(row, &layer.norm1_w, &layer.norm1_b))
            .collect::<Vec<_>>();
        let qkv = linear_batch(&normalized_rows, sequence.len(), &layer.qkv_w, &layer.qkv_b);
        let rows = selected.clone().count();
        let attention = self.attention_batch(&qkv, selected.clone());
        let projected = linear_batch(&attention, rows, &layer.out_w, &layer.out_b);
        let mut rows = selected
            .into_iter()
            .zip(projected.chunks_exact(self.width))
            .map(|(index, projected)| {
                let mut row = sequence[index].clone();
                for (left, right) in row.iter_mut().zip(projected) {
                    *left += *right;
                }
                row
            })
            .collect::<Vec<_>>();
        let normalized_rows = rows
            .iter()
            .flat_map(|row| normalized(row, &layer.norm2_w, &layer.norm2_b))
            .collect::<Vec<_>>();
        let mut hidden = linear_batch(
            &normalized_rows,
            rows.len(),
            &layer.linear1_w,
            &layer.linear1_b,
        );
        gelu_in_place(&mut hidden);
        let projected = linear_batch(&hidden, rows.len(), &layer.linear2_w, &layer.linear2_b);
        for (row, projected) in rows.iter_mut().zip(projected.chunks_exact(self.width)) {
            for (left, right) in row.iter_mut().zip(projected) {
                *left += *right;
            }
        }
        rows
    }

    fn transform(&self, sequence: &mut [Vec<f32>], layer: &TransformerLayer) {
        let normalized_rows = sequence
            .iter()
            .flat_map(|row| normalized(row, &layer.norm1_w, &layer.norm1_b))
            .collect::<Vec<_>>();
        let qkv = linear_batch(&normalized_rows, sequence.len(), &layer.qkv_w, &layer.qkv_b);
        let attention = self.attention_batch(&qkv, 0..sequence.len());
        let projected = linear_batch(&attention, sequence.len(), &layer.out_w, &layer.out_b);
        for (row, projected) in sequence.iter_mut().zip(projected.chunks_exact(self.width)) {
            row.iter_mut()
                .zip(projected)
                .for_each(|(left, right)| *left += right);
        }
        let normalized_rows = sequence
            .iter()
            .flat_map(|row| normalized(row, &layer.norm2_w, &layer.norm2_b))
            .collect::<Vec<_>>();
        let mut hidden = linear_batch(
            &normalized_rows,
            sequence.len(),
            &layer.linear1_w,
            &layer.linear1_b,
        );
        gelu_in_place(&mut hidden);
        let projected = linear_batch(&hidden, sequence.len(), &layer.linear2_w, &layer.linear2_b);
        for (row, projected) in sequence.iter_mut().zip(projected.chunks_exact(self.width)) {
            row.iter_mut()
                .zip(projected)
                .for_each(|(left, right)| *left += right);
        }
    }

    fn summarize(&self, name: usize, mode: u8, values: Vec<Vec<f32>>) -> Vec<f32> {
        match mode {
            0 => values
                .into_iter()
                .fold(vec![0.0; self.width], |mut sum, value| {
                    sum.iter_mut()
                        .zip(value)
                        .for_each(|(sum, value)| *sum += value);
                    sum
                }),
            1 => {
                let mut sequence = vec![self.summary_seed[name].clone()];
                sequence.extend(values);
                self.transform(
                    &mut sequence,
                    self.pool_layers[name.min(12)].as_ref().unwrap(),
                );
                sequence.remove(0)
            }
            2 if name == 10 => self
                .continuation_gru
                .as_ref()
                .unwrap()
                .apply(values, self.width),
            _ => unreachable!(),
        }
    }

    fn map(
        &self,
        observation: &ObservationV56,
        current: u32,
        cache: &mut EncodingCache,
    ) -> (Vec<f32>, Arc<MapEncoding>) {
        let nodes = observation.domains[MAP_NODE_DOMAIN]
            .iter()
            .filter(|row| row.scope == STATE_SCOPE)
            .collect::<Vec<_>>();
        let mut key = Vec::new();
        for domain in [MAP_NODE_DOMAIN, MAP_EDGE_DOMAIN] {
            key.extend((domain as u32).to_le_bytes());
            for row in observation.domains[domain]
                .iter()
                .filter(|row| row.scope == STATE_SCOPE)
            {
                row.u
                    .iter()
                    .for_each(|value| key.extend(value.to_le_bytes()));
                row.s
                    .iter()
                    .for_each(|value| key.extend(value.to_le_bytes()));
                row.c
                    .iter()
                    .for_each(|value| key.extend(value.to_le_bytes()));
                row.f
                    .iter()
                    .for_each(|value| key.extend(value.to_le_bytes()));
            }
        }
        let encoded = cache.maps.get(&key).cloned().unwrap_or_else(|| {
            let edges = observation.domains[MAP_EDGE_DOMAIN]
                .iter()
                .filter(|row| row.scope == STATE_SCOPE)
                .map(|row| (row, self.encode(cache, MAP_EDGE_DOMAIN, row)))
                .collect::<Vec<_>>();
            let mut encoded = nodes
                .iter()
                .map(|row| (row.u[0], self.encode(cache, MAP_NODE_DOMAIN, row)))
                .collect::<std::collections::BTreeMap<_, _>>();
            let mut levels = nodes.iter().map(|row| row.u[8]).collect::<Vec<_>>();
            levels.sort_unstable();
            levels.dedup();
            for level in levels.into_iter().rev() {
                let normalized_nodes = encoded
                    .iter()
                    .map(|(&id, value)| {
                        (
                            id,
                            normalized(value, &self.graph_norm_w, &self.graph_norm_b),
                        )
                    })
                    .collect::<std::collections::BTreeMap<_, _>>();
                let updates = nodes
                    .iter()
                    .filter(|row| row.u[8] == level && row.u[9] > 0)
                    .map(|row| {
                        let id = row.u[0];
                        let query = self.graph_query.apply(&normalized_nodes[&id]);
                        let children = edges
                            .iter()
                            .filter(|(edge, _)| edge.u[0] == id)
                            .map(|(edge, value)| {
                                self.graph_key_value
                                    .apply(&normalized_nodes[&edge.u[1]])
                                    .into_iter()
                                    .zip(self.graph_edge.apply(value))
                                    .map(|(left, right)| left + right)
                                    .collect::<Vec<_>>()
                            })
                            .collect::<Vec<_>>();
                        let mut value = encoded[&id].clone();
                        let attention = self.graph_out.apply(&self.attention(&query, &children, 0));
                        let degree = self
                            .graph_degree
                            .apply(&[row.u[9] as f32 / 8.0, (row.u[9] as f32).ln_1p() / 3.0]);
                        for column in 0..self.width {
                            value[column] += attention[column] + degree[column];
                        }
                        let hidden = dense_gelu(
                            &normalized(&value, &self.graph_ff_norm_w, &self.graph_ff_norm_b),
                            &self.graph_ff1.w,
                            &self.graph_ff1.b,
                        );
                        for (left, right) in value.iter_mut().zip(self.graph_ff2.apply(&hidden)) {
                            *left += right;
                        }
                        (id, value)
                    })
                    .collect::<Vec<_>>();
                for (id, value) in updates {
                    encoded.insert(id, value);
                }
            }
            if cache.maps.len() == 1024 {
                cache.maps.clear();
            }
            let key_values = encoded
                .values()
                .map(|value| {
                    self.graph_key_value.apply(&normalized(
                        value,
                        &self.graph_norm_w,
                        &self.graph_norm_b,
                    ))
                })
                .collect();
            let encoded = Arc::new(MapEncoding {
                nodes: encoded,
                key_values,
                current: Mutex::default(),
            });
            cache.maps.insert(key, Arc::clone(&encoded));
            encoded
        });
        let cached = encoded.current.lock().unwrap().get(&current).cloned();
        if let Some(selected) = cached {
            return (selected, encoded);
        }
        let mut selected = encoded.nodes[&current].clone();
        let query = self.graph_query.apply(&normalized(
            &selected,
            &self.graph_norm_w,
            &self.graph_norm_b,
        ));
        for (left, right) in selected
            .iter_mut()
            .zip(
                self.graph_out
                    .apply(&self.attention(&query, &encoded.key_values, 0)),
            )
        {
            *left += right;
        }
        let hidden = dense_gelu(
            &normalized(&selected, &self.graph_ff_norm_w, &self.graph_ff_norm_b),
            &self.graph_ff1.w,
            &self.graph_ff1.b,
        );
        for (left, right) in selected.iter_mut().zip(self.graph_ff2.apply(&hidden)) {
            *left += right;
        }
        encoded
            .current
            .lock()
            .unwrap()
            .insert(current, selected.clone());
        (selected, encoded)
    }

    fn collection(&self, mut values: Vec<Vec<f32>>, name: usize, collection: u32) -> Vec<Vec<f32>> {
        let mode = self.pooling[name];
        if mode == 2 {
            return values
                .into_iter()
                .map(|value| self.tag(value, 9, Some(collection)))
                .collect();
        }
        vec![self.tag(
            self.summarize(name, mode, std::mem::take(&mut values)),
            14,
            Some(collection),
        )]
    }

    fn actors(
        &self,
        observation: &ObservationV56,
        cache: &mut EncodingCache,
    ) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        let mut actors = observation.domains[ACTOR_DOMAIN]
            .iter()
            .filter(|row| row.scope == STATE_SCOPE)
            .collect::<Vec<_>>();
        actors.sort_by_key(|row| row.u[0]);
        let mut values = Vec::new();
        let mut effects_out = Vec::new();
        for actor in actors {
            let owner = actor.u[0];
            let mut value = self.encode(cache, ACTOR_DOMAIN, actor);
            for history in observation.domains[HISTORY_DOMAIN]
                .iter()
                .filter(|row| row.scope == STATE_SCOPE && row.u[0] == owner && row.u[1] == 0)
            {
                let encoded = self.encode(cache, HISTORY_DOMAIN, history);
                value
                    .iter_mut()
                    .zip(encoded)
                    .for_each(|(left, right)| *left += right);
            }
            let mut moves = observation.domains[HISTORY_DOMAIN]
                .iter()
                .filter(|row| row.scope == STATE_SCOPE && row.u[0] == owner && row.u[1] == 1)
                .collect::<Vec<_>>();
            moves.sort_by_key(|row| row.u[2]);
            let history = self.move_gru.apply(
                moves.into_iter().map(|row| {
                    let code = row.c[0] as usize;
                    self.semantic_embedding[code * self.width..][..self.width].to_vec()
                }),
                self.width,
            );
            value
                .iter_mut()
                .zip(history)
                .for_each(|(left, right)| *left += right);
            let mut effects = Vec::new();
            for domain in [POWER_DOMAIN, STATUS_DOMAIN] {
                effects.extend(
                    observation.domains[domain]
                        .iter()
                        .filter(|row| row.scope == STATE_SCOPE && row.u[0] == owner)
                        .map(|row| self.encode(cache, domain, row)),
                );
            }
            let index = if actor.u[1] == 2 { 8 } else { 9 };
            let mode = self.pooling[index];
            if mode == 4 {
                effects_out.extend(
                    effects
                        .into_iter()
                        .map(|effect| self.tag(effect, 10, Some(if index == 8 { 9 } else { 8 }))),
                );
            } else {
                let pooled =
                    self.summarize(index, if mode.is_multiple_of(2) { 0 } else { 1 }, effects);
                if mode < 2 {
                    value
                        .iter_mut()
                        .zip(pooled)
                        .for_each(|(left, right)| *left += right);
                } else {
                    effects_out.push(self.tag(pooled, 10, Some(if index == 8 { 9 } else { 8 })));
                }
            }
            value = self.tag(value, 8, None);
            layer_norm(&mut value, &self.actor_norm_w, &self.actor_norm_b);
            values.push(value);
        }
        (values, effects_out)
    }

    fn continuation_items(
        &self,
        observation: &ObservationV56,
        cache: &mut EncodingCache,
    ) -> Vec<Vec<f32>> {
        let mut rows = observation.domains[CONTINUATION_DOMAIN]
            .iter()
            .filter(|row| row.scope == STATE_SCOPE)
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| (row.u[8], row.u[2]));
        let mut items = Vec::<(u32, Vec<f32>)>::new();
        for row in rows {
            if items.last().is_none_or(|(id, _)| *id != row.u[2]) {
                items.push((row.u[2], vec![0.0; self.width]));
            }
            let encoded = self.encode(cache, CONTINUATION_DOMAIN, row);
            items
                .last_mut()
                .unwrap()
                .1
                .iter_mut()
                .zip(encoded)
                .for_each(|(left, right)| *left += right);
        }
        items.into_iter().map(|(_, value)| value).collect()
    }

    fn state_actions(
        &self,
        observation: &ObservationV56,
        cache: &mut EncodingCache,
    ) -> (Vec<f32>, Vec<Vec<f32>>) {
        let state_rows = |domain: usize| {
            observation.domains[domain]
                .iter()
                .filter(|row| row.scope == STATE_SCOPE)
                .collect::<Vec<_>>()
        };
        let run = state_rows(RUN_DOMAIN);
        let current_id = run[0].u[23];
        let (map, nodes) = self.map(observation, current_id, cache);
        let mut phase = state_rows(PHASE_DOMAIN)
            .into_iter()
            .map(|row| self.encode(cache, PHASE_DOMAIN, row))
            .collect::<Vec<_>>();
        for domain in 0..DOMAIN_NAMES.len() {
            phase.extend(
                observation.domains[domain]
                    .iter()
                    .filter(|row| row.scope == PHASE_SCOPE)
                    .map(|row| self.encode(cache, domain, row)),
            );
        }
        let generation = [
            (CARD_DOMAIN, 13, 4),
            (RELIC_DOMAIN, 14, 5),
            (ENCOUNTER_DOMAIN, 15, 6),
            (EVENT_DOMAIN, 16, 7),
        ]
        .map(|(domain, seed, role)| {
            let values = state_rows(domain)
                .into_iter()
                .filter(|row| match domain {
                    CARD_DOMAIN => row.u[0] == CARD_POOL_ZONE as u32,
                    RELIC_DOMAIN => matches!(row.u[0], 1 | 2),
                    ENCOUNTER_DOMAIN => row.u[0] <= 2,
                    EVENT_DOMAIN => row.u[0] == 0,
                    _ => false,
                })
                .map(|row| self.encode(cache, domain, row))
                .collect();
            self.tag(self.summarize(seed, self.pooling[12], values), role, None)
        });
        let mut tokens = vec![
            self.tag(self.encode(cache, RUN_DOMAIN, run[0]), 1, None),
            self.tag(self.summarize(11, self.pooling[11], phase), 2, None),
            self.tag(map, 3, None),
        ];
        tokens.extend(generation);
        let cards = state_rows(CARD_DOMAIN);
        let deck = cards
            .iter()
            .filter(|row| row.u[0] == 0)
            .map(|row| self.encode(cache, CARD_DOMAIN, row))
            .collect();
        tokens.append(&mut self.collection(deck, 1, 0));
        let mut relics = state_rows(RELIC_DOMAIN)
            .into_iter()
            .filter(|row| row.u[0] == 0)
            .map(|row| (row, self.encode(cache, RELIC_DOMAIN, row)))
            .collect::<Vec<_>>();
        let stored = cards
            .iter()
            .filter(|row| row.u[0] == PAEL_ZONE as u32)
            .map(|row| self.encode(cache, CARD_DOMAIN, row))
            .fold(vec![0.0; self.width], |mut sum, value| {
                sum.iter_mut()
                    .zip(value)
                    .for_each(|(left, right)| *left += right);
                sum
            });
        for (row, value) in &mut relics {
            if row.u[9] != 0 {
                value
                    .iter_mut()
                    .zip(&stored)
                    .for_each(|(left, right)| *left += right);
            }
        }
        tokens.append(&mut self.collection(
            relics.into_iter().map(|(_, value)| value).collect(),
            0,
            5,
        ));
        let potions = state_rows(POTION_DOMAIN)
            .into_iter()
            .filter(|row| row.u[0] == 0)
            .map(|row| self.encode(cache, POTION_DOMAIN, row))
            .collect();
        tokens.append(&mut self.collection(potions, 7, 6));
        let continuations = self.continuation_items(observation, cache);
        if self.pooling[10] == 3 {
            tokens.extend(
                continuations
                    .into_iter()
                    .map(|value| self.tag(value, 11, Some(10))),
            );
        } else {
            tokens.push(self.tag(
                self.summarize(10, self.pooling[10], continuations),
                14,
                Some(10),
            ));
        }
        tokens.extend(
            state_rows(CRYSTAL_DOMAIN)
                .into_iter()
                .map(|row| self.tag(self.encode(cache, CRYSTAL_DOMAIN, row), 12, None)),
        );
        if !state_rows(ACTOR_DOMAIN).is_empty() {
            let (actors, effects) = self.actors(observation, cache);
            tokens.extend(actors);
            for (zone, name, collection) in [(1, 5, 1), (2, 2, 2), (3, 4, 3), (4, 3, 4)] {
                let values = cards
                    .iter()
                    .filter(|row| row.u[0] == zone)
                    .map(|row| self.encode(cache, CARD_DOMAIN, row))
                    .collect();
                tokens.append(&mut self.collection(values, name, collection));
            }
            let orbs = state_rows(ORB_DOMAIN)
                .into_iter()
                .map(|row| self.encode(cache, ORB_DOMAIN, row))
                .collect();
            tokens.append(&mut self.collection(orbs, 6, 7));
            tokens.extend(effects);
        }
        let action_count = observation.candidates.len();
        for (index, candidate) in observation.candidates.iter().enumerate() {
            let mut action = self.encode_values(
                cache,
                DOMAIN_NAMES.len(),
                &candidate.c,
                &candidate.f,
                &self.action_encoder,
            );
            for domain in 0..DOMAIN_NAMES.len() {
                for row in observation.domains[domain]
                    .iter()
                    .filter(|row| row.scope == index as i32)
                {
                    let value = self.encode(cache, domain, row);
                    action
                        .iter_mut()
                        .zip(value)
                        .for_each(|(left, right)| *left += right);
                }
            }
            if candidate.u[4] != NO_NODE {
                action
                    .iter_mut()
                    .zip(&nodes.nodes[&candidate.u[4]])
                    .for_each(|(left, right)| *left += right);
            }
            action = self.tag(action, 13, None);
            layer_norm(&mut action, &self.action_norm_w, &self.action_norm_b);
            tokens.push(action);
        }
        let action_start = tokens.len() - action_count + 1;
        let mut sequence = vec![self.embedding(Semantic::TokenRole, 0).to_vec()];
        sequence.extend(tokens);
        let (last, layers) = self.global_layers.split_last().unwrap();
        for layer in layers {
            self.transform(&mut sequence, layer);
        }
        sequence = self.transformed(
            &sequence,
            last,
            std::iter::once(0).chain(action_start..sequence.len()),
        );
        for value in &mut sequence {
            layer_norm(value, &self.global_norm_w, &self.global_norm_b);
        }
        (sequence.remove(0), sequence)
    }

    #[cfg(feature = "python")]
    fn state_actions_batch(
        &self,
        observations: &[&ObservationV56],
    ) -> Vec<(Vec<f32>, Vec<Vec<f32>>)> {
        observations
            .par_iter()
            .map(|observation| {
                let index = rayon::current_thread_index().unwrap_or(0) % self.encode_caches.len();
                let mut cache = self.encode_caches[index].lock().unwrap();
                if cache.rows.len() > 65_536 {
                    cache.rows.clear();
                }
                self.state_actions(observation, &mut cache)
            })
            .collect()
    }

    fn state(&self, observation: &ObservationV56) -> Vec<f32> {
        self.state_actions(observation, &mut EncodingCache::default())
            .0
    }

    fn evaluate(
        &self,
        observation: &ObservationV56,
        temperature: f32,
        cache: &mut EncodingCache,
    ) -> io::Result<(Vec<f32>, f32, f32, Vec<f32>)> {
        let policy = self
            .policy
            .as_ref()
            .ok_or_else(|| invalid("value model has no actor head"))?;
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(invalid("invalid policy temperature"));
        }
        let (state, actions) = self.state_actions(observation, cache);
        let raw = actions
            .iter()
            .map(|action| policy.apply(action)[0] / temperature)
            .collect::<Vec<_>>();
        let normalizer = log_sum_exp(&raw);
        let scores = raw.into_iter().map(|score| score - normalizer).collect();
        let shaped = self.critic.apply(&state)[0];
        let expected = shaped + potential_value(&observation.potential, &self.potential_weights);
        Ok((
            scores,
            sigmoid(expected / self.temperature + self.bias),
            expected,
            vec![shaped],
        ))
    }

    fn evaluate_batch(
        &self,
        observations: &[&ObservationV56],
        features: &[(Vec<f32>, Vec<Vec<f32>>)],
        temperature: f32,
        rollout_temperature: Option<f32>,
        values: bool,
    ) -> io::Result<Vec<(Vec<f32>, f32, f32, Vec<f32>, Option<Vec<f32>>)>> {
        let policy = self
            .policy
            .as_ref()
            .ok_or_else(|| invalid("value model has no actor head"))?;
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(invalid("invalid policy temperature"));
        }
        let evaluate = |index: usize| {
            let observation = observations[index];
            let (state, actions) = &features[index];
            let scores = |temperature: f32| {
                let input = actions.iter().flatten().copied().collect::<Vec<_>>();
                let raw = linear_batch(&input, actions.len(), &policy.w, &policy.b)
                    .into_iter()
                    .map(|score| score / temperature)
                    .collect::<Vec<_>>();
                let normalizer = log_sum_exp(&raw);
                raw.into_iter()
                    .map(|score| score - normalizer)
                    .collect::<Vec<_>>()
            };
            let shaped = values.then(|| self.critic.apply(state)[0]);
            let expected = shaped.map_or(0.0, |value| {
                value + potential_value(&observation.potential, &self.potential_weights)
            });
            (
                scores(temperature),
                shaped.map_or(0.0, |_| sigmoid(expected / self.temperature + self.bias)),
                expected,
                shaped.into_iter().collect(),
                rollout_temperature.map(scores),
            )
        };
        #[cfg(feature = "python")]
        let output = (0..features.len()).into_par_iter().map(evaluate).collect();
        #[cfg(not(feature = "python"))]
        let output = (0..features.len()).map(evaluate).collect();
        Ok(output)
    }

    pub fn win_probability(&self, game: &Game, content: &Content) -> f32 {
        match game.phase {
            Phase::Won => return 1.0,
            Phase::Dead => return 0.0,
            _ => {}
        }
        let observation = observation_v56(game, content, self.layout, (0, 0));
        let value = self.critic.apply(&self.state(&observation))[0]
            + potential_value(&observation.potential, &self.potential_weights);
        sigmoid(value / self.temperature + self.bias)
    }
}

fn transformer_pool(index: usize, mode: u8) -> bool {
    match index {
        0..=7 | 11 | 12 => mode == 1,
        8 | 9 => matches!(mode, 1 | 3),
        10 => mode == 1,
        _ => false,
    }
}

fn valid_pooling(pooling: [u8; 13]) -> bool {
    pooling[..8].iter().all(|&mode| mode <= 2)
        && pooling[8..10].iter().all(|&mode| mode <= 4)
        && pooling[10] <= 3
        && pooling[11..].iter().all(|&mode| mode <= 1)
}

fn linear(input: &[f32], weights: &[f32], bias: &[f32]) -> Vec<f32> {
    if input.is_empty() {
        return bias.to_vec();
    }
    #[cfg(target_os = "macos")]
    {
        #[link(name = "Accelerate", kind = "framework")]
        unsafe extern "C" {
            fn cblas_sgemv(
                order: i32,
                transpose: i32,
                rows: i32,
                columns: i32,
                alpha: f32,
                matrix: *const f32,
                stride: i32,
                input: *const f32,
                input_stride: i32,
                beta: f32,
                output: *mut f32,
                output_stride: i32,
            );
        }
        let mut output = bias.to_vec();
        unsafe {
            cblas_sgemv(
                101,
                111,
                bias.len() as i32,
                input.len() as i32,
                1.0,
                weights.as_ptr(),
                input.len() as i32,
                input.as_ptr(),
                1,
                1.0,
                output.as_mut_ptr(),
                1,
            );
        }
        output
    }
    #[cfg(not(target_os = "macos"))]
    bias.iter()
        .enumerate()
        .map(|(row, &bias)| {
            weights[row * input.len()..][..input.len()]
                .iter()
                .zip(input)
                .fold(bias, |sum, (weight, value)| sum + weight * value)
        })
        .collect()
}

fn linear_batch(input: &[f32], rows: usize, weights: &[f32], bias: &[f32]) -> Vec<f32> {
    if rows < 2 {
        return linear(input, weights, bias);
    }
    #[cfg(target_os = "macos")]
    {
        #[link(name = "Accelerate", kind = "framework")]
        unsafe extern "C" {
            fn cblas_sgemm(
                order: i32,
                transpose_a: i32,
                transpose_b: i32,
                rows: i32,
                columns: i32,
                inner: i32,
                alpha: f32,
                left: *const f32,
                left_stride: i32,
                right: *const f32,
                right_stride: i32,
                beta: f32,
                output: *mut f32,
                output_stride: i32,
            );
        }
        let inner = input.len() / rows;
        let columns = bias.len();
        let mut output = bias.repeat(rows);
        unsafe {
            cblas_sgemm(
                101,
                111,
                112,
                rows as i32,
                columns as i32,
                inner as i32,
                1.0,
                input.as_ptr(),
                inner as i32,
                weights.as_ptr(),
                inner as i32,
                1.0,
                output.as_mut_ptr(),
                columns as i32,
            );
        }
        output
    }
    #[cfg(not(target_os = "macos"))]
    input
        .chunks_exact(input.len() / rows)
        .flat_map(|row| linear(row, weights, bias))
        .collect()
}

fn dense_gelu(input: &[f32], weights: &[f32], bias: &[f32]) -> Vec<f32> {
    let mut output = linear(input, weights, bias);
    gelu_in_place(&mut output);
    output
}

fn gelu_in_place(values: &mut [f32]) {
    let gelu = |value: f32, exponential: f32| {
        let x = value.abs() * std::f32::consts::FRAC_1_SQRT_2;
        let t = 1.0 / (1.0 + 0.3275911 * x);
        let erf = value.signum()
            * (1.0
                - (((((1.061_405_4 * t - 1.453_152_1) * t + 1.421_413_8) * t - 0.284_496_72) * t
                    + 0.254_829_6)
                    * t
                    * exponential));
        value * 0.5 * (1.0 + erf)
    };
    #[cfg(target_os = "macos")]
    {
        let mut exponential = values
            .iter()
            .map(|value| -0.5 * value * value)
            .collect::<Vec<_>>();
        exp_in_place(&mut exponential);
        values
            .iter_mut()
            .zip(exponential)
            .for_each(|(value, exponential)| *value = gelu(*value, exponential));
    }
    #[cfg(not(target_os = "macos"))]
    values
        .iter_mut()
        .for_each(|value| *value = gelu(*value, (-0.5 * *value * *value).exp()));
}

fn exp_in_place(values: &mut [f32]) {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "Accelerate", kind = "framework")]
        unsafe extern "C" {
            fn vvexpf(output: *mut f32, input: *const f32, count: *const i32);
        }
        let count = values.len() as i32;
        unsafe { vvexpf(values.as_mut_ptr(), values.as_ptr(), &count) };
    }
    #[cfg(not(target_os = "macos"))]
    values.iter_mut().for_each(|value| *value = value.exp());
}

fn normalized(input: &[f32], weight: &[f32], bias: &[f32]) -> Vec<f32> {
    let mut out = input.to_vec();
    layer_norm(&mut out, weight, bias);
    out
}

fn layer_norm(input: &mut [f32], weight: &[f32], bias: &[f32]) {
    normalize_block(input);
    for ((value, weight), bias) in input.iter_mut().zip(weight).zip(bias) {
        *value = *value * weight + bias;
    }
}

fn normalize_block(input: &mut [f32]) {
    let mean = input.iter().sum::<f32>() / input.len() as f32;
    let variance = input
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f32>()
        / input.len() as f32;
    let scale = (variance + 1e-5).sqrt().recip();
    input
        .iter_mut()
        .for_each(|value| *value = (*value - mean) * scale);
}

fn sigmoid(value: f32) -> f32 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

fn log_sum_exp(values: &[f32]) -> f32 {
    let maximum = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    maximum
        + values
            .iter()
            .map(|value| (value - maximum).exp())
            .sum::<f32>()
            .ln()
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn take_bytes<'a>(input: &mut &'a [u8], len: usize) -> io::Result<&'a [u8]> {
    if input.len() < len {
        return Err(invalid("truncated value model"));
    }
    let (value, rest) = input.split_at(len);
    *input = rest;
    Ok(value)
}

fn read_u32(input: &mut &[u8]) -> io::Result<u32> {
    Ok(u32::from_le_bytes(
        take_bytes(input, 4)?.try_into().unwrap(),
    ))
}

fn read_u64(input: &mut &[u8]) -> io::Result<u64> {
    Ok(u64::from_le_bytes(
        take_bytes(input, 8)?.try_into().unwrap(),
    ))
}

fn read_f32(input: &mut &[u8]) -> io::Result<f32> {
    Ok(f32::from_le_bytes(
        take_bytes(input, 4)?.try_into().unwrap(),
    ))
}

fn read_f32s(input: &mut &[u8], len: usize) -> io::Result<Vec<f32>> {
    (0..len).map(|_| read_f32(input)).collect()
}

pub(super) fn valid_model_shape(
    width: usize,
    layers: usize,
    heads: usize,
    feedforward: usize,
) -> bool {
    width > 0 && layers > 0 && heads > 0 && feedforward > 0 && width.is_multiple_of(heads)
}

#[cfg(feature = "python")]
#[path = "python.rs"]
pub(super) mod python;
