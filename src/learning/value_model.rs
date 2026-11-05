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
        let mut out = self.input(semantic, numeric, table, width);
        layer_norm(&mut out, &self.norm_w, &self.norm_b);
        out
    }

    fn input(&self, semantic: &[u32], numeric: &[f32], table: &[f32], width: usize) -> Vec<f32> {
        let mut out = self.numeric.apply(numeric);
        for &code in semantic.iter().filter(|&&code| code != 0) {
            let embedding = &table[code as usize * width..][..width];
            out.iter_mut()
                .zip(embedding)
                .for_each(|(value, embedding)| *value += embedding);
        }
        out
    }
}

struct MapEncoding {
    nodes: std::collections::BTreeMap<u32, Vec<f32>>,
}

#[derive(Default)]
struct EncodingCache {
    rows: HashMap<(u64, u64), Vec<f32>>,
    maps: HashMap<Vec<u8>, Arc<MapEncoding>>,
    shared_maps: HashMap<(usize, usize), (Arc<[DomainRow]>, Arc<[DomainRow]>, Arc<MapEncoding>)>,
    summaries: HashMap<(u64, u64), Vec<f32>>,
    move_gru: GruCache,
}

impl EncodingCache {
    fn bound(&mut self, rows: usize, width: usize) {
        if self.rows.len() > rows || self.move_gru.states.len() > rows * width {
            self.rows.clear();
            self.move_gru = GruCache::default();
        }
    }
}

#[derive(Default)]
struct GruCache {
    states: Vec<f32>,
    edges: HashMap<(usize, u32), usize>,
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

    fn step(&self, input: &[f32], state: &mut [f32], width: usize) {
        let input = self.input.apply(input);
        let hidden = self.hidden.apply(state);
        for column in 0..width {
            let reset = sigmoid(input[column] + hidden[column]);
            let update = sigmoid(input[width + column] + hidden[width + column]);
            let candidate = (input[2 * width + column] + reset * hidden[2 * width + column]).tanh();
            state[column] = (1.0 - update) * candidate + update * state[column];
        }
    }

    fn apply<T: AsRef<[f32]>>(
        &self,
        sequence: impl IntoIterator<Item = T>,
        width: usize,
    ) -> Vec<f32> {
        let mut state = vec![0.0; width];
        for value in sequence {
            self.step(value.as_ref(), &mut state, width);
        }
        state
    }

    fn apply_cached(
        &self,
        sequence: impl IntoIterator<Item = u32>,
        embeddings: &[f32],
        width: usize,
        cache: &mut GruCache,
    ) -> Vec<f32> {
        if cache.states.is_empty() {
            cache.states.resize(width, 0.0);
        }
        let mut node = 0;
        for code in sequence {
            if let Some(&next) = cache.edges.get(&(node, code)) {
                node = next;
                continue;
            }
            let mut state = cache.states[node * width..][..width].to_vec();
            self.step(
                &embeddings[code as usize * width..][..width],
                &mut state,
                width,
            );
            let next = cache.states.len() / width;
            cache.states.extend(state);
            cache.edges.insert((node, code), next);
            node = next;
        }
        cache.states[node * width..][..width].to_vec()
    }
}

pub struct ValueModel {
    layout: Layout,
    width: usize,
    heads: usize,
    pooling: [u8; 11],
    semantic_embedding: Vec<f32>,
    encoders: Vec<Option<TokenEncoderWeights>>,
    action_encoder: TokenEncoderWeights,
    map_node_embedding: Vec<f32>,
    summary_seed: Vec<Vec<f32>>,
    pool_layers: Vec<Option<TransformerLayer>>,
    pool_projections: Vec<LinearWeights>,
    pael_cards: LinearWeights,
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
    graph_out: LinearWeights,
    graph_ff_norm_w: Vec<f32>,
    graph_ff_norm_b: Vec<f32>,
    graph_ff1: LinearWeights,
    graph_ff2: LinearWeights,
    policy: Option<LinearWeights>,
    critic: LinearWeights,
    temperature: f32,
    bias: f32,
    potential: Vec<f32>,
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
            || model_version != VALUE_MODEL_VERSION
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
        let pooling = <[u8; 11]>::try_from(take_bytes(&mut input, 11)?).unwrap();
        let flags = take_bytes(&mut input, 1)?[0];
        let actor = flags & 1 != 0;
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
        let potential = read_f32s(&mut input, PUBLIC_GLOBALS)?;
        if !temperature.is_finite()
            || temperature <= 0.0
            || !bias.is_finite()
            || potential.iter().any(|weight| !weight.is_finite())
        {
            return Err(invalid("invalid value calibration"));
        }
        let semantic_embedding = read_f32s(&mut input, concept_vocab as usize * width)?;
        let map_node_embedding = read_f32s(&mut input, MAP_NODE_KINDS * width)?;
        let encoders = DOMAIN_WIDTHS
            .iter()
            .enumerate()
            .map(|(domain, &(_, _, semantic, numeric))| {
                if matches!(domain, POTION_DOMAIN | MAP_EDGE_DOMAIN) {
                    Ok(None)
                } else {
                    TokenEncoderWeights::read(&mut input, semantic, numeric, width).map(Some)
                }
            })
            .collect::<io::Result<Vec<_>>>()?;
        let action_encoder = TokenEncoderWeights::read(&mut input, ACTION_C, ACTION_F, width)?;
        let summary_seed = (0..11)
            .map(|_| read_f32s(&mut input, width))
            .collect::<io::Result<Vec<_>>>()?;
        let pool_projections = (0..3)
            .map(|_| LinearWeights::read(&mut input, width, width))
            .collect::<io::Result<Vec<_>>>()?;
        let pael_cards = LinearWeights::read_without_bias(&mut input, width, width)?;
        let mut pool_layers = Vec::with_capacity(11);
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
        let graph_out = LinearWeights::read(&mut input, width, width)?;
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
            map_node_embedding,
            summary_seed,
            pool_layers,
            pool_projections,
            pael_cards,
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
            graph_out,
            graph_ff_norm_w,
            graph_ff_norm_b,
            graph_ff1,
            graph_ff2,
            policy,
            critic,
            temperature,
            bias,
            potential,
            encode_caches: (0..16).map(|_| Mutex::default()).collect(),
        })
    }

    fn embedding(&self, semantic: Semantic, value: u32) -> &[f32] {
        let code = self.layout.semantic(semantic, value) as usize;
        &self.semantic_embedding[code * self.width..][..self.width]
    }

    #[cfg(test)]
    pub(super) fn candidate_object_key(observation: &Observation, index: usize) -> (u32, u32) {
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
        if domain == POTION_DOMAIN {
            return self.semantic_embedding[row.c[0] as usize * self.width..][..self.width]
                .to_vec();
        }
        let encoder = self.encoders[domain]
            .as_ref()
            .expect("domain has no encoder");
        if domain == MAP_NODE_DOMAIN {
            let mut value = encoder.input(&row.c, &row.f, &self.semantic_embedding, self.width);
            value
                .iter_mut()
                .zip(&self.map_node_embedding[row.u[1] as usize * self.width..][..self.width])
                .for_each(|(left, right)| *left += right);
            layer_norm(&mut value, &encoder.norm_w, &encoder.norm_b);
            return value;
        }
        self.encode_values(cache, domain, &row.c, &row.f, encoder)
    }

    fn encode_phase(&self, row: &DomainRow) -> Vec<f32> {
        self.encoders[PHASE_DOMAIN].as_ref().unwrap().input(
            &row.c,
            &row.f,
            &self.semantic_embedding,
            self.width,
        )
    }

    fn tag(&self, mut value: Vec<f32>, semantic: Semantic, index: u32) -> Vec<f32> {
        for (sum, embedding) in value.iter_mut().zip(self.embedding(semantic, index)) {
            *sum += embedding;
        }
        value
    }

    fn add_embedding(&self, value: &mut [f32], semantic: Semantic, index: u32) {
        value
            .iter_mut()
            .zip(self.embedding(semantic, index))
            .for_each(|(left, right)| *left += right);
    }

    fn card_context(&self, row: &DomainRow, mut value: Vec<f32>) -> Vec<f32> {
        if row.u[0] == DRAW_ZONE as u32 && row.u[3] > 0 {
            let semantic = if row.u[2] == 1 {
                Semantic::DrawTopPosition
            } else {
                Semantic::DrawBottomPosition
            };
            self.add_embedding(&mut value, semantic, row.u[3].saturating_sub(1).min(256));
        }
        if row.u[19] > 0 {
            self.add_embedding(
                &mut value,
                Semantic::DeckOrigin,
                row.u[19].saturating_sub(1).min(256),
            );
        }
        value
    }

    fn relic_context(&self, row: &DomainRow, mut value: Vec<f32>) -> Vec<f32> {
        if row.u[2] > 0 {
            self.add_embedding(
                &mut value,
                Semantic::RelicPosition,
                row.u[2].saturating_sub(1).min(256),
            );
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
        output: &mut [f32],
    ) {
        let rows = qkv.len() / (3 * self.width);
        let dimension = self.width / self.heads;
        let selected_len = selected.clone().count();
        #[cfg(target_os = "macos")]
        {
            #[link(name = "Accelerate", kind = "framework")]
            unsafe extern "C" {
                fn cblas_sgemm_singlecore(
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
            let mut query = Vec::new();
            let mut scores = vec![0.0; selected_len * rows];
            for head in 0..self.heads {
                let column = head * dimension;
                let (query, stride) = if dense {
                    (&qkv[column..], 3 * self.width)
                } else {
                    query.resize(selected_len * dimension, 0.0);
                    for (target, source) in query.chunks_exact_mut(dimension).zip(selected.clone())
                    {
                        target
                            .copy_from_slice(&qkv[source * 3 * self.width + column..][..dimension]);
                    }
                    (&query[..], dimension)
                };
                unsafe {
                    cblas_sgemm_singlecore(
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
                scores.chunks_exact_mut(rows).for_each(softmax);
                unsafe {
                    cblas_sgemm_singlecore(
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
        }
    }

    fn layer(&self, rows: &mut Vec<f32>, sequences: &[(usize, usize)], layer: &TransformerLayer) {
        thread_local!(static SCRATCH: std::cell::RefCell<[Vec<f32>; 4]> = Default::default());
        SCRATCH.with_borrow_mut(|[normal_buffer, qkv_buffer, attention_buffer, hidden_buffer]| {
            let width = self.width;
            let normalized = normalize_rows(rows, width, &layer.norm1_w, &layer.norm1_b, normal_buffer);
            let qkv = matmul(normalized, rows.len() / width, &layer.qkv_w, qkv_buffer);
            add_bias(qkv, &layer.qkv_b);
            let kept = sequences.iter().map(|&(length, start)| length + 1 - start).sum::<usize>();
            let attention = scratch(attention_buffer, kept * width);
            let (mut offset, mut target) = (0, 0);
            for &(length, start) in sequences {
                let count = length + 1 - start;
                self.attention_batch(
                    &qkv[offset * 3 * width..(offset + length) * 3 * width],
                    std::iter::once(0).chain(start..length),
                    &mut attention[target * width..(target + count) * width],
                );
                offset += length;
                target += count;
            }
            if kept * width != rows.len() {
                let mut offset = 0;
                *rows = sequences
                    .iter()
                    .flat_map(|&(length, start)| {
                        let sequence = &rows[offset * width..(offset + length) * width];
                        offset += length;
                        std::iter::once(0)
                            .chain(start..length)
                            .flat_map(move |index| &sequence[index * width..(index + 1) * width])
                    })
                    .copied()
                    .collect();
            }
            add_residual(rows, matmul(attention, kept, &layer.out_w, qkv_buffer), &layer.out_b);
            let normalized = normalize_rows(rows, width, &layer.norm2_w, &layer.norm2_b, normal_buffer);
            let hidden = matmul(normalized, kept, &layer.linear1_w, hidden_buffer);
            add_bias(hidden, &layer.linear1_b);
            gelu_in_place(hidden);
            add_residual(rows, matmul(hidden, kept, &layer.linear2_w, qkv_buffer), &layer.linear2_b);
        })
    }

    fn encoder(&self, sequences: &[(Vec<Vec<f32>>, usize)]) -> Vec<f32> {
        let mut rows = sequences
            .iter()
            .flat_map(|(sequence, _)| sequence.iter().flatten().copied())
            .collect::<Vec<_>>();
        let (last, layers) = self.global_layers.split_last().unwrap();
        let full = sequences
            .iter()
            .map(|(sequence, _)| (sequence.len(), 1))
            .collect::<Vec<_>>();
        for layer in layers {
            self.layer(&mut rows, &full, layer);
        }
        let selected = sequences
            .iter()
            .map(|(sequence, start)| (sequence.len(), *start))
            .collect::<Vec<_>>();
        self.layer(&mut rows, &selected, last);
        for row in rows.chunks_exact_mut(self.width) {
            layer_norm(row, &self.global_norm_w, &self.global_norm_b);
        }
        rows
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
                let mut rows = self.summary_seed[name]
                    .iter()
                    .chain(values.iter().flatten())
                    .copied()
                    .collect::<Vec<_>>();
                self.layer(
                    &mut rows,
                    &[(values.len() + 1, 1)],
                    self.pool_layers[name.min(12)].as_ref().unwrap(),
                );
                rows.truncate(self.width);
                rows
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
        observation: &Observation,
        current: u32,
        cache: &mut EncodingCache,
    ) -> (Vec<f32>, Arc<MapEncoding>) {
        let nodes = observation.domains[MAP_NODE_DOMAIN]
            .iter()
            .filter(|row| row.scope == STATE_SCOPE)
            .collect::<Vec<_>>();
        let shared = match (
            &observation.domains[MAP_NODE_DOMAIN],
            &observation.domains[MAP_EDGE_DOMAIN],
        ) {
            (DomainRows::Shared(nodes), DomainRows::Shared(edges)) => Some((nodes, edges)),
            _ => None,
        };
        let encoded = shared
            .and_then(|(nodes, edges)| {
                cache
                    .shared_maps
                    .get(&(nodes.as_ptr() as usize, edges.as_ptr() as usize))
                    .and_then(|(cached_nodes, cached_edges, encoded)| {
                        (Arc::ptr_eq(nodes, cached_nodes) && Arc::ptr_eq(edges, cached_edges))
                            .then(|| Arc::clone(encoded))
                    })
            })
            .unwrap_or_else(|| {
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
                        .collect::<Vec<_>>();
                    let mut encoded = nodes
                        .iter()
                        .map(|row| (row.u[0], self.encode(cache, MAP_NODE_DOMAIN, row)))
                        .collect::<std::collections::BTreeMap<_, _>>();
                    let mut ranks = nodes
                        .iter()
                        .map(|row| (row.u[0], 0u32))
                        .collect::<std::collections::BTreeMap<_, _>>();
                    for edge in &edges {
                        ranks.insert(edge.u[1], ranks[&edge.u[1]].max(ranks[&edge.u[0]] + 1));
                    }
                    let mut levels = ranks.values().copied().collect::<Vec<_>>();
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
                            .filter(|row| {
                                ranks[&row.u[0]] == level
                                    && edges.iter().any(|edge| edge.u[0] == row.u[0])
                            })
                            .map(|row| {
                                let id = row.u[0];
                                let query = self.graph_query.apply(&normalized_nodes[&id]);
                                let children = edges
                                    .iter()
                                    .filter(|edge| edge.u[0] == id)
                                    .map(|edge| {
                                        self.graph_key_value.apply(&normalized_nodes[&edge.u[1]])
                                    })
                                    .collect::<Vec<_>>();
                                let mut value = encoded[&id].clone();
                                let attention =
                                    self.graph_out.apply(&self.attention(&query, &children, 0));
                                for column in 0..self.width {
                                    value[column] += attention[column];
                                }
                                let hidden = dense_gelu(
                                    &normalized(
                                        &value,
                                        &self.graph_ff_norm_w,
                                        &self.graph_ff_norm_b,
                                    ),
                                    &self.graph_ff1.w,
                                    &self.graph_ff1.b,
                                );
                                for (left, right) in
                                    value.iter_mut().zip(self.graph_ff2.apply(&hidden))
                                {
                                    *left += right;
                                }
                                (id, value)
                            })
                            .collect::<Vec<_>>();
                        for (id, value) in updates {
                            encoded.insert(id, value);
                        }
                    }
                    if cache.maps.len() == 1024 || cache.shared_maps.len() == 1024 {
                        cache.maps.clear();
                        cache.shared_maps.clear();
                    }
                    let encoded = Arc::new(MapEncoding { nodes: encoded });
                    cache.maps.insert(key, Arc::clone(&encoded));
                    encoded
                });
                if let Some((nodes, edges)) = shared {
                    cache.shared_maps.insert(
                        (nodes.as_ptr() as usize, edges.as_ptr() as usize),
                        (Arc::clone(nodes), Arc::clone(edges), Arc::clone(&encoded)),
                    );
                }
                encoded
            });
        (encoded.nodes[&current].clone(), encoded)
    }

    fn collection(&self, mut values: Vec<Vec<f32>>, name: usize, collection: u32) -> Vec<Vec<f32>> {
        let mode = self.pooling[name];
        if mode == 2 {
            return values
                .into_iter()
                .map(|value| self.tag(value, Semantic::Collection, collection))
                .collect();
        }
        vec![self.tag(
            self.summarize(name, mode, std::mem::take(&mut values)),
            Semantic::Collection,
            collection,
        )]
    }

    fn bag(
        &self,
        observation: &Observation,
        cache: &mut EncodingCache,
        domain: usize,
        zone: impl Fn(u32) -> bool,
        pool: usize,
    ) -> Vec<f32> {
        let rows = observation.domains[domain]
            .iter()
            .filter(|row| row.scope == STATE_SCOPE && zone(row.u[0]))
            .collect::<Vec<_>>();
        let mut key = (0xcbf2_9ce4_8422_2325, 0x9e37_79b9_7f4a_7c15);
        for value in std::iter::once(pool as u32).chain(rows.iter().flat_map(|row| {
            row.u
                .iter()
                .copied()
                .chain(row.s.iter().map(|&value| value as u32))
        })) {
            key.0 = (key.0 ^ value as u64).wrapping_mul(0x100_0000_01b3);
            key.1 = (key.1 ^ value as u64).wrapping_mul(0x9e37_79b1_85eb_ca87);
        }
        if let Some(value) = cache.summaries.get(&key) {
            return value.clone();
        }
        let values = rows
            .into_iter()
            .map(|row| self.encode(cache, domain, row))
            .collect();
        let summary = self.summarize(0, 0, values);
        let value = self.pool_projections[pool].apply(&summary);
        cache.summaries.insert(key, value.clone());
        value
    }

    fn actors(
        &self,
        observation: &Observation,
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
            let history = self.move_gru.apply_cached(
                moves.into_iter().map(|row| row.c[0]),
                &self.semantic_embedding,
                self.width,
                &mut cache.move_gru,
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
            if owner == 1 {
                effects.extend(
                    observation.domains[CARD_DOMAIN]
                        .iter()
                        .filter(|row| {
                            row.scope == STATE_SCOPE
                                && matches!(row.u[0], NIGHTMARE_ZONE | HISTORY_COURSE_ZONE)
                        })
                        .map(|row| self.encode(cache, CARD_DOMAIN, row)),
                );
            }
            let index = if actor.u[1] == 2 { 8 } else { 9 };
            let mode = self.pooling[index];
            if mode == 4 {
                effects_out.extend(effects.into_iter().map(|effect| {
                    self.tag(effect, Semantic::Collection, if index == 8 { 9 } else { 8 })
                }));
            } else {
                let pooled =
                    self.summarize(index, if mode.is_multiple_of(2) { 0 } else { 1 }, effects);
                if mode < 2 {
                    value
                        .iter_mut()
                        .zip(pooled)
                        .for_each(|(left, right)| *left += right);
                } else {
                    effects_out.push(self.tag(
                        pooled,
                        Semantic::Collection,
                        if index == 8 { 9 } else { 8 },
                    ));
                }
            }
            layer_norm(&mut value, &self.actor_norm_w, &self.actor_norm_b);
            values.push(value);
        }
        (values, effects_out)
    }

    fn continuation_items(
        &self,
        observation: &Observation,
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
        for domain in [CARD_DOMAIN, RELIC_DOMAIN, POTION_DOMAIN] {
            for row in observation.domains[domain]
                .iter()
                .filter(|row| row.scope <= -3)
            {
                let item = (-3 - row.scope) as u32;
                let encoded = self.encode(cache, domain, row);
                let encoded = if domain == CARD_DOMAIN {
                    self.card_context(row, encoded)
                } else {
                    encoded
                };
                if let Some((_, value)) = items.iter_mut().find(|(id, _)| *id == item) {
                    value
                        .iter_mut()
                        .zip(encoded)
                        .for_each(|(left, right)| *left += right);
                }
            }
        }
        items.into_iter().map(|(_, value)| value).collect()
    }

    fn state_tokens(
        &self,
        observation: &Observation,
        cache: &mut EncodingCache,
    ) -> (Vec<Vec<f32>>, usize) {
        let state_rows = |domain: usize| {
            observation.domains[domain]
                .iter()
                .filter(|row| row.scope == STATE_SCOPE)
                .collect::<Vec<_>>()
        };
        let run = state_rows(RUN_DOMAIN);
        let current_id = run[0].u[22];
        let (map, nodes) = self.map(observation, current_id, cache);
        let mut payload = vec![0.0; self.width];
        for domain in 0..DOMAIN_NAMES.len() {
            for row in observation.domains[domain]
                .iter()
                .filter(|row| row.scope == PHASE_SCOPE)
            {
                payload
                    .iter_mut()
                    .zip(self.encode(cache, domain, row))
                    .for_each(|(left, right)| *left += right);
            }
        }
        let bags = [
            self.bag(observation, cache, RELIC_DOMAIN, |zone| zone == 1, 0),
            self.bag(
                observation,
                cache,
                ENCOUNTER_DOMAIN,
                |zone| matches!(zone, 1 | 2),
                1,
            ),
            self.bag(observation, cache, EVENT_DOMAIN, |zone| zone == 0, 2),
        ];
        let run = self.encode(cache, RUN_DOMAIN, run[0]);
        let mut tokens = Vec::new();
        for row in state_rows(PHASE_DOMAIN) {
            let mut phase = self.encode_phase(row);
            if row.u[3] != 0 {
                phase
                    .iter_mut()
                    .zip(&payload)
                    .for_each(|(left, right)| *left += right);
            }
            layer_norm(
                &mut phase,
                &self.encoders[PHASE_DOMAIN].as_ref().unwrap().norm_w,
                &self.encoders[PHASE_DOMAIN].as_ref().unwrap().norm_b,
            );
            tokens.push(phase);
        }
        tokens.push(self.tag(map, Semantic::TokenRole, 0));
        tokens.extend(bags);
        let cards = state_rows(CARD_DOMAIN);
        let deck = cards
            .iter()
            .filter(|row| row.u[0] == 0)
            .map(|row| self.card_context(row, self.encode(cache, CARD_DOMAIN, row)))
            .collect();
        tokens.append(&mut self.collection(deck, 1, 0));
        let mut relics = state_rows(RELIC_DOMAIN)
            .into_iter()
            .filter(|row| row.u[0] == 0)
            .map(|row| {
                (
                    row,
                    self.relic_context(row, self.encode(cache, RELIC_DOMAIN, row)),
                )
            })
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
        let stored = self.pael_cards.apply(&stored);
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
                    .map(|value| self.tag(value, Semantic::Collection, 10)),
            );
        } else {
            tokens.push(self.tag(
                self.summarize(10, self.pooling[10], continuations),
                Semantic::Collection,
                10,
            ));
        }
        tokens.extend(
            state_rows(CRYSTAL_DOMAIN)
                .into_iter()
                .map(|row| self.encode(cache, CRYSTAL_DOMAIN, row)),
        );
        if !state_rows(ACTOR_DOMAIN).is_empty() {
            let (actors, effects) = self.actors(observation, cache);
            tokens.extend(actors);
            for (zone, name, collection) in [(1, 5, 1), (2, 2, 2), (3, 4, 3), (4, 3, 4)] {
                let values = cards
                    .iter()
                    .filter(|row| row.u[0] == zone)
                    .map(|row| self.card_context(row, self.encode(cache, CARD_DOMAIN, row)))
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
        let scoped = scoped_rows(&observation.domains, action_count);
        for (index, candidate) in observation.candidates.iter().enumerate() {
            let mut action = self.encode_values(
                cache,
                DOMAIN_NAMES.len(),
                &candidate.c,
                &candidate.f,
                &self.action_encoder,
            );
            for &(domain, row) in &scoped[index] {
                let value = if domain == CARD_DOMAIN {
                    self.card_context(row, self.encode(cache, domain, row))
                } else {
                    self.encode(cache, domain, row)
                };
                action
                    .iter_mut()
                    .zip(value)
                    .for_each(|(left, right)| *left += right);
            }
            if candidate.u[4] != NO_NODE {
                action
                    .iter_mut()
                    .zip(&nodes.nodes[&candidate.u[4]])
                    .for_each(|(left, right)| *left += right);
            }
            layer_norm(&mut action, &self.action_norm_w, &self.action_norm_b);
            tokens.push(action);
        }
        let action_start = tokens.len() - action_count + 1;
        let mut sequence = vec![run];
        sequence.extend(tokens);
        (sequence, action_start)
    }

    fn state_actions(
        &self,
        observation: &Observation,
        cache: &mut EncodingCache,
    ) -> (Vec<f32>, Vec<Vec<f32>>) {
        let rows = self.encoder(&[self.state_tokens(observation, cache)]);
        let mut rows = rows.chunks_exact(self.width).map(<[f32]>::to_vec);
        (rows.next().unwrap(), rows.collect())
    }

    #[cfg(feature = "python")]
    fn state_actions_batch(&self, observations: &[&Observation]) -> Vec<(Vec<f32>, Vec<Vec<f32>>)> {
        observations
            .par_chunks(8)
            .flat_map_iter(|observations| {
                let index = rayon::current_thread_index().unwrap_or(0) % self.encode_caches.len();
                let mut cache = self.encode_caches[index].lock().unwrap();
                cache.bound(65_536, self.width);
                self.encoded(
                    observations
                        .iter()
                        .map(|observation| self.state_tokens(observation, &mut cache))
                        .collect(),
                )
            })
            .collect()
    }

    #[cfg(feature = "python")]
    fn encoded(
        &self,
        sequences: Vec<(Vec<Vec<f32>>, usize)>,
    ) -> std::vec::IntoIter<(Vec<f32>, Vec<Vec<f32>>)> {
        let rows = self.encoder(&sequences);
        let mut rows = rows.chunks_exact(self.width).map(<[f32]>::to_vec);
        sequences
            .iter()
            .map(|(sequence, start)| {
                let state = rows.next().unwrap();
                (state, rows.by_ref().take(sequence.len() - start).collect())
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn potential(&self, observation: &Observation) -> f32 {
        observation
            .globals
            .iter()
            .zip(&self.potential)
            .map(|(feature, weight)| feature * weight)
            .sum()
    }

    fn state(&self, observation: &Observation) -> Vec<f32> {
        self.state_actions(observation, &mut EncodingCache::default())
            .0
    }

    fn evaluate(
        &self,
        observation: &Observation,
        temperature: f32,
        cache: &mut EncodingCache,
    ) -> io::Result<(Vec<f32>, f32, f32)> {
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
        let expected = self.critic.apply(&state)[0] + self.potential(observation);
        Ok((
            scores,
            sigmoid(expected / self.temperature + self.bias),
            expected,
        ))
    }

    fn evaluate_batch(
        &self,
        observations: &[&Observation],
        features: &[(Vec<f32>, Vec<Vec<f32>>)],
        temperature: f32,
        rollout_temperature: Option<f32>,
        values: bool,
    ) -> io::Result<Vec<(Vec<f32>, f32, f32, Option<Vec<f32>>)>> {
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
            let expected =
                values.then(|| self.critic.apply(state)[0] + self.potential(observation));
            (
                scores(temperature),
                expected.map_or(0.0, |value| sigmoid(value / self.temperature + self.bias)),
                expected.unwrap_or(0.0),
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
        let observation = build_observation(game, content, self.layout, (0, 0));
        let value = self.critic.apply(&self.state(&observation))[0] + self.potential(&observation);
        sigmoid(value / self.temperature + self.bias)
    }
}

fn scoped_rows(
    domains: &[DomainRows; DOMAIN_NAMES.len()],
    action_count: usize,
) -> Vec<Vec<(usize, &DomainRow)>> {
    let mut scoped = vec![Vec::new(); action_count];
    for (domain, rows) in domains.iter().enumerate() {
        for row in rows.iter().filter(|row| row.scope >= 0) {
            if let Some(scoped) = scoped.get_mut(row.scope as usize) {
                scoped.push((domain, row));
            }
        }
    }
    scoped
}

fn transformer_pool(index: usize, mode: u8) -> bool {
    match index {
        0..=7 => mode == 1,
        8 | 9 => matches!(mode, 1 | 3),
        10 => mode == 1,
        _ => false,
    }
}

fn valid_pooling(pooling: [u8; 11]) -> bool {
    pooling[..8].iter().all(|&mode| mode <= 2)
        && pooling[8..10].iter().all(|&mode| mode <= 4)
        && pooling[10] <= 3
}

fn linear(input: &[f32], weights: &[f32], bias: &[f32]) -> Vec<f32> {
    if input.is_empty() {
        return bias.to_vec();
    }
    #[cfg(target_os = "macos")]
    {
        #[link(name = "Accelerate", kind = "framework")]
        unsafe extern "C" {
            fn cblas_sgemv_singlecore(
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
            cblas_sgemv_singlecore(
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
            fn cblas_sgemm_singlecore(
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
            cblas_sgemm_singlecore(
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

fn scratch(buffer: &mut Vec<f32>, len: usize) -> &mut [f32] {
    if buffer.len() < len {
        buffer.resize(len, 0.0);
    }
    &mut buffer[..len]
}

fn matmul<'a>(input: &[f32], rows: usize, weights: &[f32], output: &'a mut Vec<f32>) -> &'a mut [f32] {
    let inner = input.len() / rows;
    let columns = weights.len() / inner;
    let output = scratch(output, rows * columns);
    #[cfg(target_os = "macos")]
    {
        #[link(name = "Accelerate", kind = "framework")]
        unsafe extern "C" {
            fn cblas_sgemm_singlecore(
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
        unsafe {
            cblas_sgemm_singlecore(
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
                0.0,
                output.as_mut_ptr(),
                columns as i32,
            );
        }
    }
    #[cfg(not(target_os = "macos"))]
    for (target, row) in output.chunks_exact_mut(columns).zip(input.chunks_exact(inner)) {
        target.copy_from_slice(&linear(row, weights, &vec![0.0; columns]));
    }
    output
}

fn add_bias(values: &mut [f32], bias: &[f32]) {
    for row in values.chunks_exact_mut(bias.len()) {
        row.iter_mut().zip(bias).for_each(|(value, bias)| *value += bias);
    }
}

fn add_residual(rows: &mut [f32], projected: &[f32], bias: &[f32]) {
    for (row, projected) in rows.chunks_exact_mut(bias.len()).zip(projected.chunks_exact(bias.len())) {
        for ((value, projected), bias) in row.iter_mut().zip(projected).zip(bias) {
            *value += projected + bias;
        }
    }
}

fn normalize_rows<'a>(
    rows: &[f32],
    width: usize,
    weight: &[f32],
    bias: &[f32],
    output: &'a mut Vec<f32>,
) -> &'a mut [f32] {
    let output = scratch(output, rows.len());
    for (target, row) in output.chunks_exact_mut(width).zip(rows.chunks_exact(width)) {
        let (mean, scale) = moments(row);
        for (((target, value), weight), bias) in target.iter_mut().zip(row).zip(weight).zip(bias) {
            *target = (value - mean) * scale * weight + bias;
        }
    }
    output
}

fn reduce(
    values: &[f32],
    initial: f32,
    fold: impl Fn(f32, f32) -> f32,
    combine: impl Fn(f32, f32) -> f32,
) -> f32 {
    let mut lanes = [initial; 8];
    let mut chunks = values.chunks_exact(8);
    for chunk in &mut chunks {
        for (lane, &value) in lanes.iter_mut().zip(chunk) {
            *lane = fold(*lane, value);
        }
    }
    let rest = chunks.remainder().iter().fold(initial, |total, &value| fold(total, value));
    lanes.into_iter().fold(rest, combine)
}

fn moments(values: &[f32]) -> (f32, f32) {
    let add = |left, right| left + right;
    let mean = reduce(values, 0.0, add, add) / values.len() as f32;
    let squares = reduce(values, 0.0, |total, value| total + value * value, add) / values.len() as f32;
    (mean, ((squares - mean * mean).max(0.0) + 1e-5).sqrt().recip())
}

fn dense_gelu(input: &[f32], weights: &[f32], bias: &[f32]) -> Vec<f32> {
    let mut output = linear(input, weights, bias);
    gelu_in_place(&mut output);
    output
}

fn gelu_in_place(values: &mut [f32]) {
    let gelu = |value: f32| {
        let x = value.abs() * std::f32::consts::FRAC_1_SQRT_2;
        let t = 1.0 / (1.0 + 0.3275911 * x);
        let poly = (((1.061_405_4 * t - 1.453_152_1) * t + 1.421_413_8) * t - 0.284_496_72) * t
            + 0.254_829_6;
        let erf = (1.0 - poly * t * (-0.5 * value * value).exp()).copysign(value);
        value * 0.5 * (1.0 + erf)
    };
    #[cfg(target_arch = "aarch64")]
    let values = {
        use std::arch::aarch64::*;
        let mut chunks = values.chunks_exact_mut(4);
        for chunk in &mut chunks {
            unsafe {
                let value = vld1q_f32(chunk.as_ptr());
                let x = vmulq_f32(vabsq_f32(value), vdupq_n_f32(std::f32::consts::FRAC_1_SQRT_2));
                let t = vdivq_f32(vdupq_n_f32(1.0), vfmaq_f32(vdupq_n_f32(1.0), x, vdupq_n_f32(0.3275911)));
                let mut poly = vdupq_n_f32(1.061_405_4);
                for c in [-1.453_152_1, 1.421_413_8, -0.284_496_72, 0.254_829_6] {
                    poly = vfmaq_f32(vdupq_n_f32(c), poly, t);
                }
                let exponential = exp4(vmulq_f32(vmulq_f32(value, value), vdupq_n_f32(-0.5)));
                let erf = vfmsq_f32(vdupq_n_f32(1.0), vmulq_f32(poly, t), exponential);
                let erf = vbslq_f32(vdupq_n_u32(0x8000_0000), value, erf);
                let half = vmulq_f32(value, vdupq_n_f32(0.5));
                vst1q_f32(chunk.as_mut_ptr(), vmulq_f32(half, vaddq_f32(vdupq_n_f32(1.0), erf)));
            }
        }
        chunks.into_remainder()
    };
    values.iter_mut().for_each(|value| *value = gelu(*value));
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn exp4(x: std::arch::aarch64::float32x4_t) -> std::arch::aarch64::float32x4_t {
    use std::arch::aarch64::*;
    unsafe {
        let x = vmaxq_f32(x, vdupq_n_f32(-87.0));
        let shifted = vfmaq_f32(vdupq_n_f32(12_582_912.0), x, vdupq_n_f32(std::f32::consts::LOG2_E));
        let n = vsubq_f32(shifted, vdupq_n_f32(12_582_912.0));
        let r = vfmsq_f32(vfmsq_f32(x, n, vdupq_n_f32(0.693_145_75)), n, vdupq_n_f32(1.428_606_8e-6));
        let mut p = vdupq_n_f32(1.0 / 5040.0);
        for c in [1.0 / 720.0, 1.0 / 120.0, 1.0 / 24.0, 1.0 / 6.0, 0.5, 1.0, 1.0] {
            p = vfmaq_f32(vdupq_n_f32(c), p, r);
        }
        let scale = vshlq_n_u32::<23>(vreinterpretq_u32_f32(shifted));
        vreinterpretq_f32_u32(vaddq_u32(vreinterpretq_u32_f32(p), scale))
    }
}

/// Replaces `values` by exp(values - shift) and returns their sum.
fn exp_shifted(values: &mut [f32], shift: f32) -> f32 {
    #[cfg(not(target_arch = "aarch64"))]
    let mut total = 0.0;
    #[cfg(target_arch = "aarch64")]
    let (values, mut total) = {
        use std::arch::aarch64::*;
        let mut chunks = values.chunks_exact_mut(4);
        let mut sum = unsafe { vdupq_n_f32(0.0) };
        for chunk in &mut chunks {
            unsafe {
                let value = exp4(vsubq_f32(vld1q_f32(chunk.as_ptr()), vdupq_n_f32(shift)));
                vst1q_f32(chunk.as_mut_ptr(), value);
                sum = vaddq_f32(sum, value);
            }
        }
        (chunks.into_remainder(), unsafe { vaddvq_f32(sum) })
    };
    for value in values {
        *value = (*value - shift).exp();
        total += *value;
    }
    total
}

fn exp_in_place(values: &mut [f32]) {
    exp_shifted(values, 0.0);
}

fn softmax(values: &mut [f32]) {
    let scale = exp_shifted(values, reduce(values, f32::NEG_INFINITY, f32::max, f32::max)).recip();
    values.iter_mut().for_each(|value| *value *= scale);
}

fn normalized(input: &[f32], weight: &[f32], bias: &[f32]) -> Vec<f32> {
    let mut out = input.to_vec();
    layer_norm(&mut out, weight, bias);
    out
}

fn layer_norm(input: &mut [f32], weight: &[f32], bias: &[f32]) {
    let (mean, scale) = moments(input);
    for ((value, weight), bias) in input.iter_mut().zip(weight).zip(bias) {
        *value = (*value - mean) * scale * weight + bias;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_gru_matches_full_recurrence() {
        let width = 2;
        let weights = |offset| LinearWeights {
            w: (0..12)
                .map(|index| (index + offset) as f32 / 100.0)
                .collect(),
            b: (0..6).map(|index| (index + offset) as f32 / 50.0).collect(),
        };
        let gru = GruWeights {
            input: weights(1),
            hidden: weights(3),
        };
        let embeddings = vec![0.0, 0.0, 0.2, -0.1, -0.3, 0.4, 0.5, 0.6, -0.7, 0.8];
        let mut cache = GruCache::default();
        for codes in [
            vec![],
            vec![1, 2, 3],
            vec![1, 2, 3],
            vec![1, 2, 4],
            vec![2, 3],
        ] {
            let expected = gru.apply(
                codes
                    .iter()
                    .map(|&code| &embeddings[code * width..][..width]),
                width,
            );
            let actual = gru.apply_cached(
                codes.iter().map(|&code| code as u32),
                &embeddings,
                width,
                &mut cache,
            );
            assert_eq!(
                actual
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(cache.states.len() / width, 7);
    }

    #[test]
    fn scoped_rows_keep_domain_and_row_order() {
        let row = |domain, scope, marker| {
            let mut row = DomainRow::new(domain, scope);
            row.u[0] = marker;
            row
        };
        let mut domains = std::array::from_fn(|_| DomainRows::Owned(Vec::new()));
        domains[0] = DomainRows::Owned(vec![row(0, 1, 1), row(0, -1, 2), row(0, 0, 3)]);
        domains[2] = DomainRows::Owned(vec![row(2, 0, 4), row(2, 3, 5), row(2, 1, 6)]);
        let scoped = scoped_rows(&domains, 2);
        let keys = |rows: &[(usize, &DomainRow)]| {
            rows.iter()
                .map(|(domain, row)| (*domain, row.u[0]))
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&scoped[0]), vec![(0, 3), (2, 4)]);
        assert_eq!(keys(&scoped[1]), vec![(0, 1), (2, 6)]);
    }
}

#[cfg(feature = "python")]
#[path = "python.rs"]
pub(super) mod python;
