import argparse
import copy
import hashlib
import json
import math
import struct
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from queue import Empty, Full, Queue
from threading import Event

import numpy as np
import torch
from torch import nn

import sts2_sim


FEATURE_VERSION = 37
MODEL_VERSION = 39
WINNING_CAPACITY = 4096
CHANGE = "V39: unified typed state and candidate-action Transformers."
STAGES = [(0, bonus) for bonus in (24, 20, 16, 12, 8, 4, 0)] + [
    (ascension, 0) for ascension in range(1, 11)
]

_TRAINING_METAL = r"""
#include <metal_stdlib>
using namespace metal;
constant uint D = 32;
constant float SCALE = 0.1767766952966369f;

kernel void attention_forward(
    device const float *qkv, device const int *offsets, device const int *sequence,
    device float *output, device float *lse, constant uint& heads,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / D, token = group / heads, head = group % heads;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * D, query = token * 3 * width + head * D + lane;
    if (end == begin + 1) {
        output[token * width + head * D + lane] = qkv[query + 2 * width];
        if (lane == 0) lse[group] = 0.0f;
        return;
    }
    float q = qkv[query], maximum = -INFINITY, sum = 0.0f, value = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 3 * width + width + head * D;
        float score = simd_sum(q * qkv[k + lane]) * SCALE;
        float next = max(maximum, score), old = exp(maximum - next), weight = exp(score - next);
        sum = sum * old + weight;
        value = value * old + weight * qkv[k + width + lane];
        maximum = next;
    }
    output[token * width + head * D + lane] = value / sum;
    if (lane == 0) lse[group] = maximum + log(sum);
}

kernel void backward_query(
    device const float *qkv, device const int *offsets, device const int *sequence,
    device const float *output, device const float *lse, device const float *grad_output,
    device float *grad_qkv, device float *delta, constant uint& heads,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / D, token = group / heads, head = group % heads;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * D, query = token * 3 * width + head * D + lane;
    uint out = token * width + head * D + lane;
    if (end == begin + 1) {
        grad_qkv[query] = 0.0f;
        if (lane == 0) delta[group] = 0.0f;
        return;
    }
    float q = qkv[query], grad = grad_output[out];
    float correction = simd_sum(grad * output[out]), dq = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 3 * width + width + head * D;
        float probability = exp(simd_sum(q * qkv[k + lane]) * SCALE - lse[group]);
        float dp = simd_sum(grad * qkv[k + width + lane]);
        dq += probability * (dp - correction) * qkv[k + lane] * SCALE;
    }
    grad_qkv[query] = dq;
    if (lane == 0) delta[group] = correction;
}

kernel void backward_key_value(
    device const float *qkv, device const int *offsets, device const int *sequence,
    device const float *output, device const float *lse, device const float *grad_output,
    device const float *delta, device float *grad_qkv, constant uint& heads,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / D, key = group / heads, head = group % heads;
    int row = sequence[key], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * D, k = key * 3 * width + width + head * D;
    if (end == begin + 1) {
        grad_qkv[k + lane] = 0.0f;
        grad_qkv[k + width + lane] = grad_output[key * width + head * D + lane];
        return;
    }
    float value = qkv[k + width + lane], dk = 0.0f, dv = 0.0f;
    for (int token = begin; token < end; ++token) {
        uint query = token * 3 * width + head * D + lane;
        uint out = token * width + head * D + lane;
        float probability = exp(simd_sum(qkv[query] * qkv[k + lane]) * SCALE - lse[token * heads + head]);
        float grad = grad_output[out];
        float ds = probability * (simd_sum(grad * value) - delta[token * heads + head]);
        dk += ds * qkv[query] * SCALE;
        dv += probability * grad;
    }
    grad_qkv[k + lane] = dk;
    grad_qkv[k + width + lane] = dv;
}

kernel void attention_forward_bfloat(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device bfloat *output, device float *lse, constant uint& heads,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / D, token = group / heads, head = group % heads;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * D, query = token * 3 * width + head * D + lane;
    if (end == begin + 1) {
        output[token * width + head * D + lane] = qkv[query + 2 * width];
        if (lane == 0) lse[group] = 0.0f;
        return;
    }
    float q = float(qkv[query]), maximum = -INFINITY, sum = 0.0f, value = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 3 * width + width + head * D;
        float score = simd_sum(q * float(qkv[k + lane])) * SCALE;
        float next = max(maximum, score), old = exp(maximum - next), weight = exp(score - next);
        sum = sum * old + weight;
        value = value * old + weight * float(qkv[k + width + lane]);
        maximum = next;
    }
    output[token * width + head * D + lane] = bfloat(value / sum);
    if (lane == 0) lse[group] = maximum + log(sum);
}

kernel void attention_backward_query_bfloat(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device const bfloat *output, device const float *lse, device const bfloat *grad_output,
    device bfloat *grad_qkv, device float *delta, constant uint& heads,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / D, token = group / heads, head = group % heads;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * D, query = token * 3 * width + head * D + lane;
    uint out = token * width + head * D + lane;
    if (end == begin + 1) {
        grad_qkv[query] = bfloat(0.0f);
        if (lane == 0) delta[group] = 0.0f;
        return;
    }
    float q = float(qkv[query]), grad = float(grad_output[out]);
    float correction = simd_sum(grad * float(output[out])), dq = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 3 * width + width + head * D;
        float probability = exp(simd_sum(q * float(qkv[k + lane])) * SCALE - lse[group]);
        float dp = simd_sum(grad * float(qkv[k + width + lane]));
        dq += probability * (dp - correction) * float(qkv[k + lane]) * SCALE;
    }
    grad_qkv[query] = bfloat(dq);
    if (lane == 0) delta[group] = correction;
}

kernel void attention_backward_key_value_bfloat(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device const bfloat *output, device const float *lse, device const bfloat *grad_output,
    device const float *delta, device bfloat *grad_qkv, constant uint& heads,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / D, key = group / heads, head = group % heads;
    int row = sequence[key], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * D, k = key * 3 * width + width + head * D;
    if (end == begin + 1) {
        grad_qkv[k + lane] = bfloat(0.0f);
        grad_qkv[k + width + lane] = grad_output[key * width + head * D + lane];
        return;
    }
    float value = float(qkv[k + width + lane]), dk = 0.0f, dv = 0.0f;
    for (int token = begin; token < end; ++token) {
        uint query = token * 3 * width + head * D + lane;
        uint out = token * width + head * D + lane;
        float probability = exp(
            simd_sum(float(qkv[query]) * float(qkv[k + lane])) * SCALE - lse[token * heads + head]
        );
        float grad = float(grad_output[out]);
        float ds = probability * (simd_sum(grad * value) - delta[token * heads + head]);
        dk += ds * float(qkv[query]) * SCALE;
        dv += probability * grad;
    }
    grad_qkv[k + lane] = bfloat(dk);
    grad_qkv[k + width + lane] = bfloat(dv);
}

constant uint TOKEN_WIDTH = 128;
constant uint TOKEN_LANES = 32;
constant uint NUMERIC_WIDTH = 24;

kernel void token_forward(
    device const int *indices, device const int *offsets, device const float *numeric,
    device const int *numeric_row, device const float *embedding,
    device const float *numeric_weight, device const float *gamma, device const float *beta,
    device float *input, device float *output, device float *stats,
    device int *occurrence, device int *count,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint row = tid / TOKEN_LANES, base = row * TOKEN_WIDTH + lane;
    int begin = offsets[row], end = offsets[row + 1], numeric_index = numeric_row[row];
    float4 value = 0.0f;
    for (int position = begin; position < end; ++position) {
        uint source = indices[position] * TOKEN_WIDTH + lane;
        value += float4(
            embedding[source], embedding[source + TOKEN_LANES],
            embedding[source + 2 * TOKEN_LANES], embedding[source + 3 * TOKEN_LANES]
        );
    }
    if (numeric_index >= 0) {
        for (uint feature = 0; feature < NUMERIC_WIDTH; ++feature) {
            float number = numeric[numeric_index * NUMERIC_WIDTH + feature];
            value += number * float4(
                numeric_weight[lane * NUMERIC_WIDTH + feature],
                numeric_weight[(lane + TOKEN_LANES) * NUMERIC_WIDTH + feature],
                numeric_weight[(lane + 2 * TOKEN_LANES) * NUMERIC_WIDTH + feature],
                numeric_weight[(lane + 3 * TOKEN_LANES) * NUMERIC_WIDTH + feature]
            );
        }
    }
    float sum = simd_sum(value.x + value.y + value.z + value.w);
    float square = simd_sum(dot(value, value));
    float mean = sum / TOKEN_WIDTH;
    float inverse_std = rsqrt(max(0.0f, square / TOKEN_WIDTH - mean * mean) + 1e-5f);
    for (uint part = 0; part < 4; ++part) {
        uint column = lane + part * TOKEN_LANES, target = base + part * TOKEN_LANES;
        input[target] = value[part];
        output[target] = max(0.0f, (value[part] - mean) * inverse_std * gamma[column] + beta[column]);
    }
    if (lane == 0) {
        stats[row * 2] = mean;
        stats[row * 2 + 1] = inverse_std;
        count[row] = end - begin;
        for (int position = begin; position < end; ++position) occurrence[position] = row;
    }
}

kernel void token_backward_input(
    device const float *input, device const float *gamma, device const float *beta,
    device const float *stats, device const float *grad_output, device float *grad_input,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint row = tid / TOKEN_LANES, base = row * TOKEN_WIDTH + lane;
    float mean = stats[row * 2], inverse_std = stats[row * 2 + 1];
    float4 normalized, grad;
    for (uint part = 0; part < 4; ++part) {
        uint column = lane + part * TOKEN_LANES, target = base + part * TOKEN_LANES;
        normalized[part] = (input[target] - mean) * inverse_std;
        float activation = normalized[part] * gamma[column] + beta[column];
        grad[part] = activation > 0 ? grad_output[target] * gamma[column] : 0.0f;
    }
    float sum = simd_sum(grad.x + grad.y + grad.z + grad.w);
    float product = simd_sum(dot(grad, normalized));
    for (uint part = 0; part < 4; ++part) {
        grad_input[base + part * TOKEN_LANES] = (
            grad[part] - sum / TOKEN_WIDTH - normalized[part] * product / TOKEN_WIDTH
        ) * inverse_std;
    }
}

kernel void token_backward_norm_partial(
    device const float *input, device const float *gamma, device const float *beta,
    device const float *stats, device const float *grad_output,
    device float *partial, constant uint& rows, constant uint& block_rows,
    uint tid [[thread_position_in_grid]]) {
    uint block = tid / TOKEN_WIDTH, column = tid % TOKEN_WIDTH;
    uint begin = block * block_rows, end = min(rows, begin + block_rows);
    float dg = 0.0f, db = 0.0f;
    for (uint row = begin; row < end; ++row) {
        uint index = row * TOKEN_WIDTH + column;
        float normalized = (input[index] - stats[row * 2]) * stats[row * 2 + 1];
        float activation = normalized * gamma[column] + beta[column];
        float grad = activation > 0 ? grad_output[index] : 0.0f;
        dg += grad * normalized;
        db += grad;
    }
    partial[block * 2 * TOKEN_WIDTH + column] = dg;
    partial[(block * 2 + 1) * TOKEN_WIDTH + column] = db;
}

kernel void token_backward_norm_reduce(
    device const float *partial, device float *grad_gamma, device float *grad_beta,
    constant uint& blocks,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_threadgroup]]) {
    threadgroup float gamma_sum[128], beta_sum[128];
    uint column = tid / TOKEN_WIDTH;
    float dg = 0.0f, db = 0.0f;
    for (uint block = lane; block < blocks; block += TOKEN_WIDTH) {
        dg += partial[block * 2 * TOKEN_WIDTH + column];
        db += partial[(block * 2 + 1) * TOKEN_WIDTH + column];
    }
    gamma_sum[lane] = dg;
    beta_sum[lane] = db;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (uint stride = 64; stride; stride /= 2) {
        if (lane < stride) {
            gamma_sum[lane] += gamma_sum[lane + stride];
            beta_sum[lane] += beta_sum[lane + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    if (lane == 0) {
        grad_gamma[column] = gamma_sum[0];
        grad_beta[column] = beta_sum[0];
    }
}

"""


class _RaggedAttention(torch.autograd.Function):
    library = None

    @staticmethod
    def forward(ctx, qkv, offsets, sequence, heads):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        output = qkv.new_empty((len(qkv), qkv.shape[1] // 3))
        lse = torch.empty((len(qkv), heads), dtype=torch.float32, device=qkv.device)
        threads = len(qkv) * heads * 32
        kernel = (_RaggedAttention.library.attention_forward_bfloat
                  if qkv.dtype == torch.bfloat16 else _RaggedAttention.library.attention_forward)
        kernel(
            qkv, offsets, sequence, output, lse, heads, threads=threads, group_size=32
        )
        ctx.save_for_backward(qkv, offsets, sequence, output, lse)
        ctx.heads = heads
        ctx.bfloat = qkv.dtype == torch.bfloat16
        return output

    @staticmethod
    def backward(ctx, grad_output):
        qkv, offsets, sequence, output, lse = ctx.saved_tensors
        grad_qkv = torch.empty_like(qkv)
        delta = torch.empty_like(lse)
        threads = len(qkv) * ctx.heads * 32
        args = qkv, offsets, sequence, output, lse, grad_output.contiguous()
        query = (_RaggedAttention.library.attention_backward_query_bfloat
                 if ctx.bfloat else _RaggedAttention.library.backward_query)
        key_value = (_RaggedAttention.library.attention_backward_key_value_bfloat
                     if ctx.bfloat else _RaggedAttention.library.backward_key_value)
        query(
            *args, grad_qkv, delta, ctx.heads, threads=threads, group_size=32
        )
        key_value(
            *args, delta, grad_qkv, ctx.heads, threads=threads, group_size=32
        )
        return grad_qkv, None, None, None


class _FusedTokens(torch.autograd.Function):
    @staticmethod
    def forward(ctx, indices, offsets, numeric, numeric_index, numeric_row,
                embedding, numeric_weight, gamma, beta):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        rows = len(offsets) - 1
        input = embedding.new_empty((rows, 128))
        output = torch.empty_like(input)
        stats = embedding.new_empty((rows, 2))
        occurrence = torch.empty_like(indices)
        count = torch.empty(rows, dtype=torch.int32, device=embedding.device)
        _RaggedAttention.library.token_forward(
            indices, offsets, numeric, numeric_row, embedding, numeric_weight, gamma, beta,
            input, output, stats, occurrence, count, threads=rows * 32, group_size=32,
        )
        ctx.save_for_backward(
            indices, offsets, numeric, numeric_index, embedding, gamma, beta,
            input, stats, occurrence, count,
        )
        return output

    @staticmethod
    def backward(ctx, grad_output):
        indices, offsets, numeric, numeric_index, embedding, gamma, beta, input, stats, occurrence, count = ctx.saved_tensors
        grad_output = grad_output.contiguous()
        grad_input = torch.empty_like(input)
        grad_gamma = torch.empty_like(gamma)
        grad_beta = torch.empty_like(beta)
        _RaggedAttention.library.token_backward_input(
            input, gamma, beta, stats, grad_output, grad_input,
            threads=len(input) * 32, group_size=32,
        )
        blocks = (len(input) + 127) // 128
        partial = torch.empty((blocks, 2, 128), dtype=torch.float32, device=input.device)
        _RaggedAttention.library.token_backward_norm_partial(
            input, gamma, beta, stats, grad_output, partial, len(input), 128,
            threads=blocks * 128, group_size=128,
        )
        _RaggedAttention.library.token_backward_norm_reduce(
            partial, grad_gamma, grad_beta, blocks,
            threads=128 * 128, group_size=128,
        )
        empty = torch.empty(0, dtype=torch.int32, device=input.device)
        grad_embedding = torch.ops.aten._embedding_bag_backward(
            grad_input, indices, offsets, occurrence, count, empty, len(embedding),
            False, 0, False, None, -1,
        )
        grad_numeric = grad_input[numeric_index].T @ numeric
        return None, None, None, None, None, grad_embedding, grad_numeric, grad_gamma, grad_beta


def device():
    if torch.backends.mps.is_available():
        return torch.device("mps")
    if torch.cuda.is_available():
        return torch.device("cuda")
    return torch.device("cpu")


class Agent(nn.Module):
    def __init__(self, layout, width=128, layers=2, heads=4, feedforward=256, action_layers=1):
        super().__init__()
        self.layout = dict(layout)
        if self.layout["version"] != FEATURE_VERSION:
            raise ValueError("incompatible token layout")
        if min(width, layers, heads, feedforward, action_layers) < 1 or width % heads:
            raise ValueError("invalid architecture")
        self.width, self.layers, self.heads, self.feedforward = width, layers, heads, feedforward
        self.action_layers = action_layers
        self.character_start = self.layout["character_start"]
        vocab = [
            self.layout["collections"], self.layout["kinds"], self.layout["ids"],
            self.layout["owners"], self.layout["positions"], self.layout["card_types"],
            self.layout["enchantments"], self.layout["afflictions"],
        ]
        sizes = [min(size, 257) for size in vocab] + [
            max(2, math.ceil((size - 1) / 256) + 1) for size in vocab
        ]
        sizes = torch.tensor(sizes)
        offsets = nn.functional.pad(sizes.cumsum(0), (1, 0))[:-1]
        self.token_sizes = sizes.numpy().astype(np.int32)
        self.token_offsets = offsets.numpy().astype(np.int32)
        self.embedding = nn.Embedding(int(sizes.sum()), width)
        self.register_buffer("embedding_sizes", sizes, persistent=False)
        self.register_buffer("embedding_offsets", offsets, persistent=False)
        with torch.no_grad():
            self.embedding.weight[offsets] = 0
        self.numeric = nn.Linear(self.layout["numeric"], width, bias=False)
        self.token_norm = nn.LayerNorm(width)
        layer = nn.TransformerEncoderLayer(
            width, heads, feedforward, dropout=0, batch_first=True, norm_first=True, activation="relu"
        )
        self.state_transformer = nn.TransformerEncoder(layer, layers, enable_nested_tensor=False)
        self.state_state = nn.Parameter(torch.randn(width) * 0.02)
        self.action_value = nn.Linear(self.layout["action_values"], width)
        self.action_norm = nn.LayerNorm(width)
        action_layer = nn.TransformerEncoderLayer(
            width, heads, feedforward, dropout=0, batch_first=True, norm_first=True, activation="relu"
        )
        self.decision_transformer = nn.TransformerEncoder(
            action_layer, action_layers, enable_nested_tensor=False
        )
        self.decision_state = nn.Parameter(torch.randn(width) * 0.02)
        self.policy = nn.Sequential(nn.Linear(width, width), nn.ReLU(), nn.Linear(width, 1))
        self.value = nn.Sequential(nn.Linear(width, width), nn.ReLU(), nn.Linear(width, 1))
        self.progress_value = nn.Sequential(nn.Linear(width, width), nn.ReLU(), nn.Linear(width, 1))
        nn.init.normal_(self.policy[-1].weight, std=0.01)
        nn.init.zeros_(self.policy[-1].bias)
        for head in (self.value, self.progress_value):
            nn.init.zeros_(head[-1].weight)
            nn.init.zeros_(head[-1].bias)

    def load_state_dict(self, state_dict, strict=True, assign=False):
        if "embedding.weight" not in state_dict:
            state_dict = state_dict.copy()
            keys = [f"embedding_{part}.{index}.weight" for part in ("low", "high") for index in range(8)]
            state_dict["embedding.weight"] = torch.cat([state_dict.pop(key) for key in keys])
        return super().load_state_dict(state_dict, strict, assign=assign)

    def encode_tokens(self, tokens):
        indices, offsets, numeric, numeric_index, numeric_row = tokens
        if indices.device.type == "mps" and self.width == 128 and self.layout["numeric"] == 24:
            return _FusedTokens.apply(
                indices, offsets, numeric, numeric_index, numeric_row,
                self.embedding.weight, self.numeric.weight, self.token_norm.weight, self.token_norm.bias,
            )
        encoded = nn.functional.embedding_bag(
            indices, self.embedding.weight, offsets, mode="sum", include_last_offset=True
        )
        encoded = encoded.index_add(0, numeric_index, self.numeric(numeric).to(encoded.dtype))
        return torch.relu(self.token_norm(encoded))

    def encode_state(self, encoded, index):
        offsets, destination, sequence, row, position = index
        if encoded.device.type != "mps" or self.width != self.heads * 32:
            lengths = offsets[1:] - offsets[:-1]
            valid = torch.arange(int(lengths.max()), device=encoded.device)[None] < lengths[:, None]
            current = encoded.new_zeros((len(lengths), valid.shape[1], self.width))
            current[:, 0] = self.state_state
            if len(encoded):
                current[row, position] = encoded
            return self.state_transformer(current, src_key_padding_mask=~valid)[:, 0]
        current = encoded.new_zeros((len(sequence), self.width))
        current[offsets[:-1]] = self.state_state
        current[destination] = encoded
        for layer in self.state_transformer.layers:
            qkv = nn.functional.linear(
                layer.norm1(current), layer.self_attn.in_proj_weight, layer.self_attn.in_proj_bias
            )
            attended = _RaggedAttention.apply(qkv, offsets, sequence, layer.self_attn.num_heads)
            current = current + layer.dropout1(layer.self_attn.out_proj(attended))
            current = current + layer.dropout2(layer.linear2(layer.dropout(
                layer.activation(layer.linear1(layer.norm2(current)))
            )))
        if self.state_transformer.norm is not None:
            current = self.state_transformer.norm(current)
        return current[offsets[:-1]]

    def forward(self, metadata, state_tokens, state_index, action_values, action_tokens, action_index,
                return_state=False):
        state = self.encode_state(self.encode_tokens(state_tokens), state_index)
        action_row, action_flat, token_action, count, actions, offsets, destination, sequence = action_index
        encoded = self.encode_tokens(action_tokens)
        action = encoded.new_zeros((len(action_values), self.width))
        if len(encoded):
            action = action.index_add(0, token_action, encoded)
        action = action / count.sqrt()[:, None]
        action = torch.relu(self.action_norm(self.action_value(action_values) + action))
        if action.device.type == "mps" and self.width == self.heads * 32:
            current = action.new_zeros((len(sequence), self.width))
            current[offsets[:-1]] = self.decision_state
            current[offsets[:-1] + 1] = state
            current[destination] = action
            for layer in self.decision_transformer.layers:
                qkv = nn.functional.linear(
                    layer.norm1(current), layer.self_attn.in_proj_weight, layer.self_attn.in_proj_bias
                )
                attended = _RaggedAttention.apply(qkv, offsets, sequence, layer.self_attn.num_heads)
                current = current + layer.dropout1(layer.self_attn.out_proj(attended))
                current = current + layer.dropout2(layer.linear2(layer.dropout(
                    layer.activation(layer.linear1(layer.norm2(current)))
                )))
            if self.decision_transformer.norm is not None:
                current = self.decision_transformer.norm(current)
            context, action = current[offsets[:-1]], current[destination]
        else:
            valid = torch.zeros((len(state), actions), dtype=torch.bool, device=action.device)
            valid.view(-1)[action_flat] = True
            represented = action.new_zeros((len(state) * actions, self.width)).index_copy(
                0, action_flat, action
            ).reshape(len(state), actions, self.width)
            padded = torch.cat((
                self.decision_state.expand(len(state), 1, -1), state[:, None], represented
            ), 1)
            current = self.decision_transformer(
                padded, src_key_padding_mask=~nn.functional.pad(valid, (2, 0), value=True)
            )
            context, action = current[:, 0], current[:, 2:].flatten(0, 1)[action_flat]
        scores = self.policy(action).squeeze(-1)
        policy = scores.new_zeros(len(state) * actions).scatter(0, action_flat, scores).reshape(len(state), actions)
        output = (
            policy,
            self.value(context).squeeze(-1),
            self.progress_value(context).squeeze(-1),
        )
        return (*output, context) if return_state else output


def architecture(model):
    return {
        "width": model.width, "layers": model.layers, "action_layers": model.action_layers,
        "heads": model.heads,
        "feedforward": model.feedforward,
        "globals": model.layout["globals"], "value_heads": ["win_logit", "progress"],
        "winning_reservoir": WINNING_CAPACITY,
    }


def predict(model, inputs, precision):
    kind = inputs[0].device.type
    if precision == "bf16" and kind not in ("mps", "cuda"):
        raise ValueError("bf16 requires MPS or CUDA")
    with torch.autocast(kind, dtype=torch.bfloat16, enabled=precision == "bf16"):
        output = model(*inputs[:6])
    return tuple(value.float() for value in output)


def token_data(categorical, numeric, target, model):
    fields = model.layout["categorical"]
    categorical = np.rint(categorical).astype(np.int32)
    present = categorical != 0
    values = np.maximum(categorical, 1) - 1
    indices = np.concatenate((
        np.minimum(values % 256 + 1, model.token_sizes[:fields] - 1) + model.token_offsets[:fields],
        np.minimum(values // 256 + 1, model.token_sizes[fields:] - 1) + model.token_offsets[fields:],
    ), 1)
    present = np.concatenate((present, present), 1)
    count = present.sum(1)
    numeric_index = np.flatnonzero(np.any(numeric != 0, axis=1))
    numeric_row = np.full(len(numeric), -1, np.int32)
    numeric_row[numeric_index] = np.arange(len(numeric_index), dtype=np.int32)
    return tuple(torch.as_tensor(value, device=target) for value in (
        indices[present], np.r_[0, np.cumsum(count, dtype=np.int32)].astype(np.int32),
        numeric[numeric_index].astype(np.float32), numeric_index, numeric_row,
    ))


def packed_state(categorical, numeric, row, batch, target, model):
    lengths = np.bincount(row, minlength=batch).astype(np.int32) + 1
    offsets = np.r_[0, np.cumsum(lengths, dtype=np.int32)].astype(np.int32)
    starts = np.repeat(np.cumsum(lengths - 1) - (lengths - 1), lengths - 1)
    position = np.arange(len(row), dtype=np.int32) - starts + 1
    destination = offsets[row] + position
    sequence = np.repeat(np.arange(batch, dtype=np.int32), lengths)
    return token_data(categorical, numeric, target, model), tuple(
        torch.as_tensor(value, device=target)
        for value in (offsets, destination, sequence, row, position)
    )


def tensors(observation, target, model):
    state = np.asarray(observation[1])
    state_mask = np.asarray(observation[2], bool)
    state_row = np.repeat(np.arange(len(state)), state_mask.sum(1))
    state = state[state_mask]
    state, state_index = packed_state(
        state[:, :model.layout["categorical"]], state[:, model.layout["categorical"]:],
        state_row, len(observation[1]), target, model,
    )
    legal = np.asarray(observation[6], bool)
    represented = np.asarray(observation[7], bool)
    represented[~represented.any(1), 0] = True
    action_row, action_position = np.nonzero(represented)
    action_lengths = np.bincount(action_row, minlength=len(legal)).astype(np.int32)
    action_offsets = np.r_[0, np.cumsum(action_lengths + 2, dtype=np.int32)].astype(np.int32)
    action_rank = np.arange(len(action_row)) - np.repeat(
        np.cumsum(action_lengths) - action_lengths, action_lengths
    )
    action_destination = action_offsets[action_row] + action_rank + 2
    action_sequence = np.repeat(np.arange(len(legal), dtype=np.int32), action_lengths + 2)
    token_mask = np.asarray(observation[5], bool)[represented]
    count = token_mask.sum(1).clip(min=1).astype(np.float32)
    token_action = np.repeat(np.arange(len(action_row)), token_mask.sum(1))
    action_tokens = np.asarray(observation[4])[represented][token_mask]
    actions = legal.shape[1]
    return (
        torch.as_tensor(observation[0], dtype=torch.float32, device=target),
        state,
        state_index,
        torch.as_tensor(np.asarray(observation[3])[represented], dtype=torch.float32, device=target),
        token_data(
            action_tokens[:, :model.layout["categorical"]],
            action_tokens[:, model.layout["categorical"]:],
            target, model,
        ),
        tuple(torch.as_tensor(value, device=target) for value in (
            action_row, action_row * actions + action_position, token_action, count,
        )) + (actions,) + tuple(torch.as_tensor(value, device=target) for value in (
            action_offsets, action_destination, action_sequence,
        )),
        torch.as_tensor(legal, dtype=torch.bool, device=target),
    )


def split_tokens(tokens):
    return tokens[:, :8].astype(np.uint32), tokens[:, 8:].astype(np.float16)


def pack(observation, index):
    state = np.asarray(observation[1][index])[np.asarray(observation[2][index], bool)]
    legal = np.asarray(observation[6][index], bool)
    represented = np.asarray(observation[7][index], bool)
    if not represented.any():
        represented[0] = True
    values = np.asarray(observation[3][index, represented], np.float16)
    rows = [
        np.asarray(tokens)[np.asarray(mask, bool)]
        for tokens, mask in zip(observation[4][index, represented], observation[5][index, represented])
    ]
    offsets = np.r_[0, np.cumsum([len(row) for row in rows])].astype(np.uint32)
    action_tokens = np.concatenate(rows) if offsets[-1] else np.zeros((0, 32), np.float32)
    state_categorical, state_numeric = split_tokens(state)
    action_categorical, action_numeric = split_tokens(action_tokens)
    return (
        np.asarray(observation[0][index], np.float16), state_categorical, state_numeric, values,
        action_categorical, action_numeric, offsets, legal[represented],
    )


def unpack(rows, target, model):
    batch = len(rows)
    state_lengths = np.asarray([len(row[1]) for row in rows])
    action_lengths = np.asarray([len(row[3]) for row in rows])
    widths = np.concatenate([np.diff(row[6]) for row in rows]).astype(np.intp)
    actions = max(1, action_lengths.max())
    globals_ = np.asarray([row[0] for row in rows], np.float32)
    state_row = np.repeat(np.arange(batch), state_lengths)
    state, state_index = packed_state(
        np.concatenate([row[1] for row in rows]), np.concatenate([row[2] for row in rows]),
        state_row, batch, target, model,
    )
    action_row = np.repeat(np.arange(batch), action_lengths)
    action_position = np.arange(action_lengths.sum()) - np.repeat(
        np.cumsum(action_lengths) - action_lengths, action_lengths
    )
    action_offsets = np.r_[0, np.cumsum(action_lengths + 2, dtype=np.int32)].astype(np.int32)
    action_destination = action_offsets[action_row] + action_position + 2
    action_sequence = np.repeat(np.arange(batch, dtype=np.int32), action_lengths + 2)
    legal = np.zeros((batch, actions), bool)
    for index, row in enumerate(rows):
        legal[index, :len(row[7])] = row[7]
    return (
        torch.as_tensor(globals_, device=target),
        state,
        state_index,
        torch.as_tensor(np.concatenate([row[3] for row in rows]), dtype=torch.float32, device=target),
        token_data(
            np.concatenate([row[4] for row in rows]), np.concatenate([row[5] for row in rows]),
            target, model,
        ),
        tuple(torch.as_tensor(value, device=target) for value in (
            action_row, action_row * actions + action_position,
            np.repeat(np.arange(len(widths)), widths), widths.clip(min=1).astype(np.float32),
        )) + (actions,) + tuple(torch.as_tensor(value, device=target) for value in (
            action_offsets, action_destination, action_sequence,
        )),
        torch.as_tensor(legal, device=target),
    )


def minibatch_scores(rows, model):
    scores = []
    for row in rows:
        state = len(row[1]) + 1
        actions = len(row[3]) + 2
        scores.append((len(row[1]) + len(row[3])) * model.width
                      + model.layers * model.heads * state ** 2
                      + model.action_layers * model.heads * actions ** 2)
    return scores


def action_bands(rows):
    return np.maximum(0, np.ceil(np.log2([
        max(1, len(row[3]) + 1) for row in rows
    ])) - 4).astype(np.int8)


class WinningReservoir:
    def __init__(self, capacity, envs):
        self.capacity = capacity
        self.pending_capacity = max(1, capacity // envs)
        self.pending = [[] for _ in range(envs)]
        self.pending_seen = [0] * envs
        self.rows = []
        self.seen = self.wins = self.skipped = 0

    def record(self, rows, choices, log_probabilities, policies, values, rng):
        for index, (row, choice, log_probability, policy, value) in enumerate(
            zip(rows, choices, log_probabilities, policies, values)
        ):
            episode = self.pending[index]
            self.pending_seen[index] += 1
            item = (row, int(choice), float(log_probability), 1.0 - float(value),
                    np.asarray(policy[:len(row[3])], np.float16))
            if len(episode) < self.pending_capacity:
                episode.append(item)
            else:
                slot = int(rng.integers(self.pending_seen[index]))
                if slot < self.pending_capacity:
                    episode[slot] = item

    def finish(self, indices, won, rng):
        added = 0
        for index, victory in zip(indices, won):
            episode = self.pending[index]
            if victory:
                self.wins += 1
                for row, action, probability, advantage, policy in episode:
                    if advantage <= 0:
                        self.skipped += 1
                        continue
                    self.seen += 1
                    item = (row, action, probability, min(advantage, 1.0), policy)
                    if len(self.rows) < self.capacity:
                        self.rows.append(item)
                        added += 1
                    else:
                        slot = int(rng.integers(self.seen))
                        if slot < self.capacity:
                            self.rows[slot] = item
                            added += 1
            episode.clear()
            self.pending_seen[index] = 0
        return added

    def discard(self, indices):
        for index in indices:
            self.pending[index].clear()
            self.pending_seen[index] = 0

    def clear(self):
        self.pending = [[] for _ in self.pending]
        self.pending_seen = [0] * len(self.pending_seen)
        self.rows.clear()
        self.seen = self.wins = self.skipped = 0

    def state_dict(self):
        return {"rows": list(self.rows), "seen": self.seen, "wins": self.wins, "skipped": self.skipped}

    def load_state_dict(self, state):
        self.rows = [row if len(row) == 5 else (*row, None)
                     for row in state.get("rows", ())][:self.capacity]
        self.seen = max(len(self.rows), state.get("seen", 0))
        self.wins = state.get("wins", 0)
        self.skipped = state.get("skipped", 0)

    def by_character(self, character_start):
        groups = [[] for _ in range(5)]
        for index, (row, *_rest) in enumerate(self.rows):
            character = int(np.asarray(row[0])[character_start : character_start + 5].argmax())
            groups[character].append(index)
        return groups

    def sample(self, count, rng, character_start):
        groups = self.by_character(character_start)
        pools = [rng.permutation([
            index for index in group if self.rows[index][4] is not None
        ]).tolist() for group in groups]
        count = min(count, sum(map(len, pools)))
        selected = []
        while len(selected) < count:
            for character in rng.permutation(5):
                if pools[character]:
                    selected.append(pools[character].pop())
                    if len(selected) == count:
                        break
        return [self.rows[index] for index in selected]


def act(model, observation, target, sample, precision, generator=None):
    with torch.no_grad():
        inputs = tensors(observation, target, model)
        logits, value_logit, progress_value = predict(model, inputs, precision)
        legal = inputs[6].clone()
        legal[~legal.any(1), 0] = True
        masked = logits.masked_fill(~legal, -torch.inf)
        distribution = torch.distributions.Categorical(logits=masked)
        choice = (
            torch.multinomial(masked.softmax(-1).cpu(), 1, generator=generator).squeeze(1).to(target)
            if sample else masked.argmax(1)
        )
        assert legal.gather(1, choice[:, None]).all()
    return (choice.cpu().numpy(), distribution.log_prob(choice).cpu().numpy(),
            distribution.logits.cpu().numpy(), value_logit.sigmoid().cpu().numpy(),
            progress_value.cpu().numpy())


def bands(values):
    if not values:
        values = [0]
    points = np.quantile(values, [0, .01, .05, .1, .25, .5, .75, .9, .95, .99, 1])
    return dict(zip(("min", "p01", "p05", "p10", "p25", "median", "p75", "p90", "p95", "p99", "max"), map(float, points))) | {"mean": float(np.mean(values))}


def metric_means(metrics):
    result = {}
    pending = []
    for key, values in metrics.items():
        if not values:
            result[key] = 0
        elif torch.is_tensor(values[0]):
            pending.append((key, values))
        else:
            result[key] = float(np.mean(values))
    if pending:
        values = torch.cat([torch.stack(rows) for _, rows in pending]).cpu().tolist()
        start = 0
        for key, rows in pending:
            result[key] = float(np.mean(values[start : start + len(rows)]))
            start += len(rows)
    return result


def summaries(episodes):
    return [
        {
            "episodes": len(rows), "wins": sum(row[0] for row in rows),
            "win_rate": sum(row[0] for row in rows) / max(1, len(rows)),
            "floor_mean": float(np.mean([row[1] for row in rows])) if rows else 0,
            "floor_bands": bands([row[1] for row in rows]),
            "caps": sum(row[2] or row[3] or row[4] for row in rows),
            "step_caps": sum(row[2] for row in rows),
            "combat_caps": sum(row[3] for row in rows),
            "empty_actions": sum(row[4] for row in rows),
        }
        for rows in episodes
    ]


def cuts(done, episode_steps, combat_steps, still_combat, legal, max_steps, max_combat_steps):
    step = episode_steps >= max_steps
    combat = (combat_steps >= max_combat_steps) & still_combat
    empty = ~legal.any(1)
    cut = (step | combat | empty) & ~done
    return cut, step & cut, combat & cut, empty & cut


def sampling_steps(produced, budget, envs, limit):
    return max(0, min(limit, (budget - produced + envs - 1) // envs))


class RolloutCollector:
    def __init__(self, args, sampler_session, stage, reservoir, iteration=0):
        ascension, bonus = STAGES[stage]
        self.args = args
        self.env = sts2_sim.Batch(
            args.envs, args.training_seed + sampler_session * 10_000_000, None,
            ascension=ascension,
        )
        self.env.set_training_bonus(bonus)
        self.observation = self.env.observe_tokens()
        self.episode_steps = np.zeros(args.envs, np.int32)
        self.combat_steps = np.zeros(args.envs, np.int32)
        self.max_floor = np.asarray(
            [(row[0] - 1) * 17 + row[1] for row in self.env.stats()], np.float32
        )
        self.reservoir = WinningReservoir(reservoir.capacity, args.envs)
        self.reservoir.load_state_dict(reservoir.state_dict())
        self.rng = np.random.default_rng(args.seed + sampler_session)
        self.torch_rng = torch.Generator().manual_seed(args.seed + sampler_session)
        self.iteration, self.sampler_session, self.stage = iteration, sampler_session, stage
        self.trajectories = [None] * args.envs
        self.action_history = [[] for _ in range(args.envs)]

    def trace_empty(self, kind, indices, characters, stats):
        seeds = self.env.seeds()
        with (Path(self.args.output) / "empty-actions.jsonl").open("a") as output:
            for index in indices:
                output.write(json.dumps({
                    "kind": kind, "sampler_session": self.sampler_session,
                    "iteration": self.iteration, "env": index, "seed": seeds[index],
                    "character": int(characters[index]), "stage": self.stage,
                    "stats": list(stats[index]), "actions": self.action_history[index],
                }) + "\n")

    def collect(self, model, target, precision, deadline, steps, version, stop=None):
        args = self.args
        finished = []
        episodes = [[] for _ in range(5)]
        discarded_steps = orphan_empty_actions = sampled_steps = winning_added = 0
        collect_seconds = 0.0
        for _ in range(steps):
            if time.monotonic() >= deadline or stop and stop.is_set():
                break
            self.iteration += 1
            empty = ~np.asarray(self.observation[6], bool).any(1)
            if empty.any():
                reset = np.flatnonzero(empty).tolist()
                stale_characters = np.asarray(self.observation[0])[
                    :, model.character_start : model.character_start + 5
                ].argmax(1)
                self.trace_empty("orphan", reset, stale_characters, self.env.stats())
                orphan_empty_actions += len(reset)
                self.reservoir.discard(reset)
                for index in reset:
                    if self.trajectories[index] is not None:
                        discarded_steps += len(self.trajectories[index]["rows"])
                    self.trajectories[index] = None
                    self.action_history[index].clear()
                self.env.reset(reset, 0.0)
                self.episode_steps[reset] = 0
                self.combat_steps[reset] = 0
                self.max_floor[reset] = 1
                self.observation = self.env.observe_tokens()
            step_started = time.monotonic()
            characters = np.asarray(self.observation[0])[
                :, model.character_start : model.character_start + 5
            ].argmax(1)
            current_potential = np.asarray(self.observation[8], np.float32)
            choice, log_probability, policy, value, progress_value = act(
                model, self.observation, target, True, precision, self.torch_rng
            )
            for index, action in enumerate(choice):
                self.action_history[index].append(int(action))
            step_rows = [pack(self.observation, index) for index in range(args.envs)]
            self.reservoir.record(step_rows, choice, log_probability, policy, value, self.rng)
            progress_floor = self.max_floor / 52
            in_combat = np.asarray([row[4] == 1 for row in self.env.stats()])
            self.combat_steps = np.where(in_combat, self.combat_steps + 1, 0)
            raw_reward, done, _ = self.env.step(choice.tolist())
            raw_reward = np.array(raw_reward, np.float32, copy=True)
            done = np.asarray(done, bool)
            sampled_steps += args.envs
            self.episode_steps += 1
            next_observation = self.env.observe_tokens((~done).tolist())
            stats = self.env.stats()
            floors = np.asarray([(row[0] - 1) * 17 + row[1] for row in stats], np.float32)
            next_max_floor = np.maximum(np.maximum(self.max_floor, floors), 52 * raw_reward)
            still_combat = np.asarray([row[4] == 1 for row in stats])
            truncated, step_truncated, combat_truncated, empty_actions = cuts(
                done, self.episode_steps, self.combat_steps, still_combat,
                np.asarray(next_observation[6], bool), args.max_steps, args.max_combat_steps,
            )
            boundary = done | truncated
            if empty_actions.any():
                reset = np.flatnonzero(empty_actions).tolist()
                self.trace_empty("post_step", reset, characters, stats)
            progress_reward = (next_max_floor - self.max_floor) / 52
            self.max_floor = next_max_floor
            shaped_reward = raw_reward + np.asarray(next_observation[8]) - current_potential
            for index, row in enumerate(step_rows):
                trajectory = self.trajectories[index]
                if trajectory is None:
                    trajectory = {key: [] for key in (
                        "rows", "choices", "old_log", "values", "progress_values",
                        "progress_floors", "win_rewards", "shaped_rewards",
                        "progress_rewards", "terminals", "characters", "versions",
                    )}
                    trajectory["started"] = step_started
                    self.trajectories[index] = trajectory
                for key, item in (
                    ("rows", row), ("choices", choice[index]),
                    ("old_log", log_probability[index]), ("values", value[index]),
                    ("progress_values", progress_value[index]),
                    ("progress_floors", progress_floor[index]),
                    ("win_rewards", raw_reward[index]),
                    ("shaped_rewards", shaped_reward[index]),
                    ("progress_rewards", progress_reward[index]),
                    ("terminals", done[index]), ("characters", characters[index]),
                    ("versions", version),
                ):
                    trajectory[key].append(item)
            if boundary.any():
                reset = np.flatnonzero(boundary).tolist()
                wins = [bool(done[index] and stats[index][4] == 12) for index in reset]
                winning_added += self.reservoir.finish(reset, wins, self.rng)
                for index in reset:
                    episodes[characters[index]].append((
                        int(done[index] and stats[index][4] == 12),
                        (stats[index][0] - 1) * 17 + stats[index][1],
                        int(step_truncated[index] and truncated[index]),
                        int(combat_truncated[index] and truncated[index]),
                        int(empty_actions[index]),
                        self.iteration,
                    ))
                    if done[index]:
                        self.trajectories[index]["completion_seconds"] = (
                            time.monotonic() - self.trajectories[index].pop("started")
                        )
                        finished.append(self.trajectories[index])
                    else:
                        discarded_steps += len(self.trajectories[index]["rows"])
                    self.trajectories[index] = None
                    self.action_history[index].clear()
                self.env.reset(reset, 0.0)
                self.episode_steps[reset] = 0
                self.combat_steps[reset] = 0
                self.max_floor[reset] = 1
                next_observation = self.env.observe_tokens()
            self.observation = next_observation
            collect_seconds += time.monotonic() - step_started
            if finished:
                break
        return {
            "trajectories": finished,
            "episodes": episodes,
            "iteration": self.iteration,
            "orphan_empty_actions": orphan_empty_actions, "winning_added": winning_added,
            "collect_seconds": collect_seconds, "discarded_steps": discarded_steps,
            "sampled_steps": sampled_steps,
        }

    def run(self, model, models, samples, stop, deadline, budget):
        version = produced = 0
        def empty():
            return {
                "trajectories": [], "episodes": [[] for _ in range(5)],
                "iteration": self.iteration, "orphan_empty_actions": 0,
                "winning_added": 0, "collect_seconds": 0.0,
                "discarded_steps": 0, "sampled_steps": 0,
            }
        pending = empty()
        while produced < budget and time.monotonic() < deadline and not stop.is_set():
            latest = None
            while True:
                try:
                    latest = models.get_nowait()
                except Empty:
                    break
            if latest:
                version, state = latest
                model.load_state_dict(state)
            steps = sampling_steps(produced, budget, self.args.envs, self.args.sampler_steps)
            if not steps:
                break
            result = self.collect(
                model, torch.device("cpu"), "fp32", deadline, steps, version, stop
            )
            pending["trajectories"].extend(result["trajectories"])
            for target, rows in zip(pending["episodes"], result["episodes"]):
                target.extend(rows)
            for key in (
                "orphan_empty_actions", "winning_added", "collect_seconds",
                "discarded_steps", "sampled_steps",
            ):
                pending[key] += result[key]
            pending["iteration"] = result["iteration"]
            if not pending["trajectories"]:
                continue
            produced += sum(len(trajectory["rows"]) for trajectory in pending["trajectories"])
            pending["reservoir"] = self.reservoir.state_dict()
            while not stop.is_set():
                try:
                    samples.put((version, pending), timeout=.1)
                    pending = empty()
                    break
                except Full:
                    pass
        pending["discarded_steps"] += sum(
            len(trajectory["rows"]) for trajectory in self.trajectories
            if trajectory is not None
        ) + sum(len(trajectory["rows"]) for trajectory in pending["trajectories"])
        pending["trajectories"].clear()
        active = [index for index, trajectory in enumerate(self.trajectories) if trajectory is not None]
        self.reservoir.discard(active)
        pending["reservoir"] = self.reservoir.state_dict()
        pending["iteration"] = self.iteration
        return version, pending


class ExperienceDataset:
    def __init__(self):
        self.rows = []
        self.data = {
            "action": np.empty(0, np.int64), "old": np.empty(0, np.float32),
            "advantage": np.empty(0, np.float32), "progress_advantage": np.empty(0, np.float32),
            "returns": np.empty(0, np.float32), "progress_returns": np.empty(0, np.float32),
            "character": np.empty(0, np.int8), "priority": np.empty(0, np.float32),
            "version": np.empty(0, np.int64), "action_band": np.empty(0, np.int8),
        }
        self.seen = self.stale_dropped = self.ratio_dropped = self.kl_dropped = 0

    def __len__(self):
        return len(self.rows)

    def add(self, result, args, model):
        trajectories = result["trajectories"]
        if not trajectories:
            return 0
        fields = (
            "rows", "choices", "old_log", "values", "progress_values",
            "progress_floors", "win_rewards", "shaped_rewards", "progress_rewards",
            "terminals", "characters", "versions",
        )
        rows = [item for trajectory in trajectories for item in trajectory["rows"]]
        reward = np.asarray([
            item for trajectory in trajectories for item in trajectory["win_rewards"]
        ], np.float32)
        shaped = np.asarray([
            item for trajectory in trajectories for item in trajectory["shaped_rewards"]
        ], np.float32)
        progress_reward = np.asarray([
            item for trajectory in trajectories for item in trajectory["progress_rewards"]
        ], np.float32)
        value = np.asarray([
            item for trajectory in trajectories for item in trajectory["values"]
        ], np.float32)
        progress_value = np.asarray([
            item for trajectory in trajectories for item in trajectory["progress_values"]
        ], np.float32)
        progress_floor = np.asarray([
            item for trajectory in trajectories for item in trajectory["progress_floors"]
        ], np.float32)
        remaining_progress = progress_value - progress_floor
        terminal = np.asarray([
            item for trajectory in trajectories for item in trajectory["terminals"]
        ], bool)
        advantage = np.zeros_like(reward); value_advantage = np.zeros_like(reward)
        progress_value_advantage = np.zeros_like(reward); progress_advantage = np.zeros_like(reward)
        end = 0
        for trajectory in trajectories:
            length = len(trajectory["rows"])
            if not length or any(len(trajectory[key]) != length for key in fields):
                raise ValueError("invalid trajectory fields")
            start, end = end, end + length
            if not terminal[end - 1] or terminal[start:end - 1].any():
                raise ValueError("invalid trajectory terminal")
            gae = value_gae = progress_gae = next_value = next_progress = 0.0
            for step in reversed(range(start, end)):
                live = not terminal[step]
                gae = shaped[step] + live * next_value - value[step] + args.gae_lambda * live * gae
                value_gae = reward[step] + live * next_value - value[step] + args.gae_lambda * live * value_gae
                progress_gae = (
                    progress_reward[step] + args.progress_gamma * live * next_progress
                    - remaining_progress[step]
                    + args.progress_gamma * args.gae_lambda * live * progress_gae
                )
                advantage[step] = gae; value_advantage[step] = value_gae
                progress_value_advantage[step] = progress_gae
                progress_advantage[step] = progress_gae
                next_value = value[step]; next_progress = remaining_progress[step]
        returns = np.clip(value_advantage + value, 0, 1)
        progress_returns = progress_value_advantage + remaining_progress + progress_floor
        priority = 1 + np.abs(advantage) + np.abs(progress_advantage) + 4 * terminal + 4 * (returns > .5)
        values = {
            "action": np.asarray([
                item for trajectory in trajectories for item in trajectory["choices"]
            ], np.int64),
            "old": np.asarray([
                item for trajectory in trajectories for item in trajectory["old_log"]
            ], np.float32),
            "advantage": advantage, "progress_advantage": progress_advantage,
            "returns": returns, "progress_returns": progress_returns,
            "character": np.asarray([
                item for trajectory in trajectories for item in trajectory["characters"]
            ], np.int8),
            "priority": priority.astype(np.float32),
            "version": np.asarray([
                item for trajectory in trajectories for item in trajectory["versions"]
            ], np.int64),
            "action_band": action_bands(rows),
        }
        self.rows.extend(rows)
        for key, value in values.items():
            self.data[key] = np.concatenate((self.data[key], value))
        self.seen += len(rows)
        return len(rows)

    def prune(self, version, lag, limit=None):
        stale = np.flatnonzero(self.data["version"] < version - lag)
        if limit is not None:
            stale = stale[:max(0, limit)]
        self.stale_dropped += len(stale)
        self.discard(stale)
        return len(stale)

    def discard(self, indices):
        if not len(indices):
            return
        keep = np.ones(len(self), bool); keep[indices] = False
        self.rows = [row for row, selected in zip(self.rows, keep) if selected]
        for key in self.data:
            self.data[key] = self.data[key][keep]

    def sample(self, size, rng):
        size = min(size, len(self))
        groups = self.data["action_band"]
        buckets, counts = np.unique(groups, return_counts=True)
        eligible = buckets[counts >= min(size, max(8, size // 2))]
        if not len(eligible):
            eligible = buckets
        weights = np.asarray([
            self.data["priority"][groups == bucket].sum() for bucket in eligible
        ])
        bucket = rng.choice(eligible, p=weights / weights.sum())
        pool = np.flatnonzero(groups == bucket)
        size = min(size, len(pool))
        selected = []
        for rank, character in enumerate(rng.permutation(5)):
            count = size // 5 + (rank < size % 5)
            group = pool[self.data["character"][pool] == character]
            take = min(count, len(group))
            if take:
                priority = np.sqrt(self.data["priority"][group])
                selected.extend(rng.choice(group, take, False, priority / priority.sum()).tolist())
        if len(selected) < size:
            remaining = np.setdiff1d(pool, selected, assume_unique=False)
            priority = np.sqrt(self.data["priority"][remaining])
            selected.extend(rng.choice(
                remaining, size - len(selected), False, priority / priority.sum()
            ).tolist())
        rng.shuffle(selected)
        return np.asarray(selected)


def train_stream(model, optimizer, args, sampler_session, stage, target, deadline, budget,
                 base_decisions, base_auxiliary, run_started, reservoir, progress_active,
                 save_report, save_step):
    ascension, bonus = STAGES[stage]
    collector = RolloutCollector(args, sampler_session, stage, reservoir, base_decisions // args.envs)
    dataset = ExperienceDataset()
    episodes = [[] for _ in range(5)]
    losses = {key: [] for key in ("mean_advantage", "policy_loss", "value_loss", "progress_value_loss", "progress_beta", "entropy", "kl", "clip_fraction", "winning_loss", "winning_kl")}
    pipeline = [
        "Continuous CPU actor → complete terminal trajectories",
        "Bounded queue → policy-lag and action-ratio freshness filters",
        "CPU-prefetched, character-balanced, advantage-prioritized, action-banded batches",
        f"{model.layers}-layer card and {model.action_layers}-layer action-set transformers → policy and value heads",
        "Asynchronous clipped PPO + KL-gated terminal winning replay on the accelerator",
    ]
    last_batch = None
    winning_behavior_drift = 0
    decisions = discarded_steps = handled = sampled = attempted = trained = updates = windows = 0
    winning_added = winning_replayed = winning_rejected = orphan_empty_actions = 0
    winning_replayed_characters = [0] * 5
    winning_reanchored_characters = [0] * 5
    trajectory_lengths = []
    trajectory_policy_spans = []
    trajectory_seconds = []
    collect_seconds = update_seconds = 0.0
    observed_kl = observed_clip = 0.0
    latest_sampler_version = dataset_peak = 0
    latest_sampler_iteration = base_decisions // args.envs
    policy_lags = []
    reported_episodes = [0] * 5
    reported_metrics = {key: 0 for key in losses}
    reported_steps = reported_trajectories = 0
    reported_seconds = 0.0
    started = time.monotonic()
    rng = np.random.default_rng(args.seed + sampler_session + 1_000_000_000)
    models = Queue(maxsize=1); samples = Queue(maxsize=2); stop = Event()
    executor = ThreadPoolExecutor(max_workers=1)
    packer = ThreadPoolExecutor(max_workers=1)
    future = executor.submit(
        collector.run, copy.deepcopy(model).cpu().eval(), models, samples, stop,
        deadline, budget,
    )

    def publish():
        if stop.is_set() or not models.empty():
            return
        models.put_nowait((updates, {
            key: value.detach().cpu().clone() for key, value in model.state_dict().items()
        }))

    def ingest(item):
        nonlocal decisions, discarded_steps, sampled, collect_seconds, winning_added, orphan_empty_actions, latest_sampler_version, latest_sampler_iteration, dataset_peak
        version, result = item
        latest_sampler_version = version
        latest_sampler_iteration = result["iteration"]
        decisions += dataset.add(result, args, model)
        sampled += result["sampled_steps"]
        discarded_steps += result["discarded_steps"]
        for trajectory in result["trajectories"]:
            trajectory_lengths.append(len(trajectory["rows"]))
            trajectory_policy_spans.append(
                max(trajectory["versions"]) - min(trajectory["versions"])
            )
            trajectory_seconds.append(trajectory["completion_seconds"])
        dataset_peak = max(dataset_peak, len(dataset))
        reservoir.load_state_dict(result["reservoir"])
        for target_episodes, collected in zip(episodes, result["episodes"]):
            target_episodes.extend(collected)
        collect_seconds += result["collect_seconds"]
        winning_added += result["winning_added"]
        orphan_empty_actions += result["orphan_empty_actions"]

    def upload(value):
        if torch.is_tensor(value):
            return value.to(target, non_blocking=True)
        if isinstance(value, tuple):
            return tuple(upload(item) for item in value)
        return value

    def reserve_batch():
        boundary = min(next_report, next_save) - (base_decisions + handled)
        index = dataset.sample(min(args.batch, boundary), rng)
        rows = [dataset.rows[i] for i in index]
        values = {key: dataset.data[key][index].copy() for key in dataset.data}
        action_band = int(values["action_band"][0])
        assert np.all(values["action_band"] == action_band)
        dataset.discard(index)
        replay_limit = len(index) // 9
        candidates = reservoir.sample(
            min(len(reservoir.rows), 4 * replay_limit), rng, model.character_start
        ) if replay_limit else []
        candidate_action_bands = action_bands([sample[0] for sample in candidates]) if candidates else []
        replay = [sample for sample, replay_action_band in zip(
            candidates, candidate_action_bands
        ) if replay_action_band <= action_band][:replay_limit]
        packed = packer.submit(
            unpack, rows + [sample[0] for sample in replay], torch.device("cpu"), model
        )
        return rows, values, action_band, replay, packed

    def report():
        nonlocal observed_kl, observed_clip, reported_steps, reported_seconds, reported_trajectories
        if last_batch:
            rows, action, old = last_batch
            inputs = unpack(rows, target, model)
            action = torch.as_tensor(action, device=target)
            old = torch.as_tensor(old, device=target)
            model.eval()
            with torch.no_grad():
                logits = predict(model, inputs, args.precision)[0]
                distribution = torch.distributions.Categorical(
                    logits=logits.masked_fill(~inputs[6], -torch.inf)
                )
                log_ratio = distribution.log_prob(action) - old
                ratio = log_ratio.exp()
                observed_kl = float((ratio - 1 - log_ratio).mean())
                observed_clip = float(((ratio - 1).abs() > args.clip).float().mean())
            model.train()
        recent_by_character = [rows[reported_episodes[index]:] for index, rows in enumerate(episodes)]
        recent = [episode for rows in recent_by_character for episode in rows]
        reported_episodes[:] = map(len, episodes)
        elapsed = time.monotonic() - run_started
        segment_elapsed = time.monotonic() - started
        recent_metrics = metric_means({
            key: values[reported_metrics[key]:]
            for key, values in losses.items() if len(values) > reported_metrics[key]
        })
        recent_lengths = trajectory_lengths[reported_trajectories:]
        recent_spans = trajectory_policy_spans[reported_trajectories:]
        recent_seconds = trajectory_seconds[reported_trajectories:]
        reported_trajectories = len(trajectory_lengths)
        for key, values in losses.items():
            reported_metrics[key] = len(values)
        replay_fraction = winning_replayed / max(1, trained + winning_replayed)
        assert replay_fraction <= .1 + 1e-9
        point = {
            "steps": base_decisions + handled, "sampled_decisions": base_decisions + sampled,
            "accepted_decisions": base_decisions + decisions,
            "iteration": latest_sampler_iteration,
            "updates": updates, "episodes": len(recent),
            "wins": sum(row[0] for row in recent), "caps": sum(row[2] or row[3] or row[4] for row in recent),
            "seconds": elapsed,
            "decisions_per_second": (handled - reported_steps) / max(1e-9, segment_elapsed - reported_seconds),
            "trajectory_floors": [(row[5], row[1], character)
                                  for character, rows in enumerate(recent_by_character) for row in rows],
            "step_caps": sum(row[2] for row in recent), "combat_caps": sum(row[3] for row in recent),
            "empty_actions": sum(row[4] for row in recent),
            "collect_seconds": collect_seconds, "update_seconds": update_seconds,
            "floor_bands": bands([row[1] for row in recent]),
            "dataset_rows": len(dataset), "dataset_peak": dataset_peak, "dataset_seen": dataset.seen,
            "dataset_attempted": attempted, "dataset_trained": trained,
            "dataset_stale_dropped": dataset.stale_dropped,
            "dataset_ratio_dropped": dataset.ratio_dropped, "dataset_kl_dropped": dataset.kl_dropped,
            "discarded_steps": discarded_steps,
            "discarded_step_fraction": discarded_steps / max(1, sampled),
            "completed_trajectories": len(trajectory_lengths),
            "trajectory_length_mean": float(np.mean(recent_lengths)) if recent_lengths else 0,
            "trajectory_length_max": max(recent_lengths, default=0),
            "trajectory_policy_span_mean": float(np.mean(recent_spans)) if recent_spans else 0,
            "trajectory_policy_span_max": max(recent_spans, default=0),
            "trajectory_completion_seconds_mean": float(np.mean(recent_seconds)) if recent_seconds else 0,
            "trajectory_completion_seconds_max": max(recent_seconds, default=0),
            "policy_version": updates, "sampler_version": latest_sampler_version,
            "policy_lag_mean": float(np.mean(policy_lags)) if policy_lags else 0,
            "winning_reservoir": len(reservoir.rows), "winning_seen": reservoir.seen,
            "winning_character_rows": list(map(len, reservoir.by_character(model.character_start))),
            "winning_episodes": reservoir.wins, "winning_added": winning_added,
            "winning_skipped": reservoir.skipped, "winning_replayed": winning_replayed,
            "winning_rejected_kl": winning_rejected,
            "winning_replayed_characters": winning_replayed_characters,
            "winning_reanchored_characters": winning_reanchored_characters,
            "winning_replay_fraction": replay_fraction, "winning_loss_weight": .1,
            "winning_collection_drift": float(winning_behavior_drift),
            "observed_kl": observed_kl, "observed_clip_fraction": observed_clip,
            **recent_metrics,
        }
        reported_steps = handled
        reported_seconds = segment_elapsed
        point["characters"] = summaries([rows[-args.promotion_window:] for rows in episodes])
        save_report(point, pipeline, windows)

    next_report = base_decisions + args.report_decisions
    next_save = base_decisions + args.save_decisions
    promotion_ready = False
    pending = None
    try:
        while True:
            sampler_done = future.done() and samples.empty()
            if pending is None:
                if len(dataset) < args.batch and not sampler_done:
                    try:
                        ingest(samples.get(timeout=.1))
                    except Empty:
                        continue
                    sampler_done = future.done() and samples.empty()
                absolute = base_decisions + handled
                boundary = min(next_report, next_save) - absolute
                stale = dataset.prune(updates, args.max_policy_lag, boundary)
                handled += stale
                absolute = base_decisions + handled
                if absolute >= next_save:
                    save_step(absolute)
                    next_save += args.save_decisions
                if absolute >= next_report:
                    windows += 1
                    report()
                    next_report += args.report_decisions
                    recent = [rows[-args.promotion_window:] for rows in episodes]
                    promotion_ready |= stage + 1 < len(STAGES) and all(
                        len(rows) >= args.promotion_window
                        and sum(row[0] for row in rows) / len(rows) >= args.promote_win_rate
                        for rows in recent
                    )
                    if promotion_ready:
                        stop.set()
                    continue
                if not len(dataset):
                    if sampler_done:
                        break
                    continue
                if len(dataset) < args.batch and not sampler_done:
                    continue
                pending = reserve_batch()
            update_started = time.monotonic()
            rows, values, fresh_action_band, replay, packed = pending
            pending = None
            reserved = len(rows)
            version_fresh = values["version"] >= updates - args.max_policy_lag
            if not version_fresh.all():
                packed.result()
                dataset.stale_dropped += int((~version_fresh).sum())
                rows = [row for row, keep in zip(rows, version_fresh) if keep]
                values = {key: value[version_fresh] for key, value in values.items()}
                replay = replay[:len(rows) // 9]
                packed = packer.submit(
                    unpack, rows + [sample[0] for sample in replay], torch.device("cpu"), model
                ) if rows else None
            handled += reserved
            attempted += len(rows)
            policy_lags.extend((updates - values["version"]).tolist())
            if not rows:
                continue
            unpack_started = time.monotonic()
            inputs = upload(packed.result())
            unpack_seconds = time.monotonic() - unpack_started
            sampler_done = future.done() and samples.empty()
            boundary = min(next_report, next_save) - (base_decisions + handled)
            enough = len(dataset) >= args.batch or sampler_done and len(dataset)
            versions_ready = not len(dataset) or np.all(
                dataset.data["version"] >= updates + 1 - args.max_policy_lag
            )
            if boundary > 0 and enough and versions_ready:
                pending = reserve_batch()
            action = torch.as_tensor(values["action"], device=target)
            old = torch.as_tensor(values["old"], device=target)
            forward_started = time.monotonic()
            all_logits, all_prediction, all_progress_prediction = predict(model, inputs, args.precision)
            forward_seconds = time.monotonic() - forward_started
            backward_seconds = 0.0
            logits = all_logits[:len(rows)]
            prediction = all_prediction[:len(rows)]
            progress_prediction = all_progress_prediction[:len(rows)]
            legal = inputs[6][:len(rows)]
            distribution = torch.distributions.Categorical(logits=logits.masked_fill(~legal, -torch.inf))
            log_ratio = distribution.log_prob(action) - old
            fresh = log_ratio.abs() <= args.max_log_ratio
            mask = fresh.to(log_ratio.dtype)
            fresh_count = mask.sum()
            denominator = fresh_count.clamp_min(1)
            safe_log_ratio = torch.where(fresh, log_ratio, torch.zeros_like(log_ratio))
            ratio = safe_log_ratio.exp()
            kl = ((ratio - 1 - safe_log_ratio) * mask).sum().div(denominator).detach()
            progress_beta = 0 if not progress_active else 1 - .9 * min(
                1, (base_auxiliary + handled) / args.progress_decisions
            )
            advantages = torch.as_tensor(values["advantage"], device=target)
            progress_advantages = torch.as_tensor(values["progress_advantage"], device=target)
            characters = torch.as_tensor(values["character"], device=target)
            normalized = []
            for advantage_values in (advantages, progress_advantages):
                result = advantage_values
                for character in range(5):
                    selected = fresh & (characters == character)
                    selected_float = selected.to(advantage_values.dtype)
                    count = selected_float.sum().clamp_min(1)
                    mean = (advantage_values * selected_float).sum() / count
                    std = (((advantage_values - mean).square() * selected_float).sum() / count).sqrt()
                    result = torch.where(selected, (advantage_values - mean) / (std + 1e-8), result)
                normalized.append(result)
            batch_advantage = normalized[0] + progress_beta * normalized[1]
            batch_returns = torch.as_tensor(values["returns"], device=target)
            batch_progress_returns = torch.as_tensor(values["progress_returns"], device=target)
            policy_loss = -(torch.minimum(
                ratio * batch_advantage,
                ratio.clamp(1 - args.clip, 1 + args.clip) * batch_advantage,
            ) * mask).sum() / denominator
            value_loss = (nn.functional.binary_cross_entropy_with_logits(
                prediction, batch_returns, reduction="none"
            ) * mask).sum() / denominator
            progress_value_loss = ((progress_prediction - batch_progress_returns).square() * mask).sum() / denominator
            entropy = (distribution.entropy() * mask).sum() / denominator
            progress = min(1, (base_auxiliary + handled) / args.progress_decisions)
            entropy_weight = args.entropy_end + (args.entropy_start - args.entropy_end) * (1 - progress)
            loss = policy_loss + args.value_weight * (
                value_loss + (progress_beta > 0) * progress_value_loss
            ) - entropy_weight * entropy
            replay_valid = replay_eligible = None
            if replay:
                replay_logits = all_logits[len(rows):]
                replay_value = all_prediction[len(rows):]
                replay_legal = inputs[6][len(rows):]
                replay_masked = replay_logits.masked_fill(~replay_legal, -torch.inf)
                replay_action = torch.as_tensor([sample[1] for sample in replay], device=target)
                reference = np.full(replay_masked.shape, -np.inf, np.float32)
                for row, sample in zip(reference, replay):
                    row[:len(sample[4])] = sample[4]
                reference = torch.as_tensor(reference, device=target)
                replay_distribution = torch.distributions.Categorical(logits=replay_masked)
                reference_distribution = torch.distributions.Categorical(logits=reference)
                replay_kl = torch.distributions.kl_divergence(reference_distribution, replay_distribution)
                replay_eligible = torch.arange(len(replay), device=target) < fresh_count.to(torch.int64) // 9
                replay_valid = replay_eligible & (replay_kl <= args.target_kl)
                replay_weight = replay_valid.to(replay_value.dtype)
                replay_denominator = replay_weight.sum().clamp_min(1)
                replay_log_probability = replay_distribution.log_prob(replay_action)
                reference_log_probability = reference_distribution.log_prob(replay_action)
                replay_log_ratio = torch.where(
                    replay_valid, replay_log_probability - reference_log_probability,
                    torch.zeros_like(replay_log_probability),
                )
                replay_ratio = replay_log_ratio.exp()
                replay_advantage = (1 - replay_value.sigmoid()).detach()
                winning_loss = -.1 * (torch.minimum(
                    replay_ratio * replay_advantage,
                    replay_ratio.clamp(1 - args.clip, 1 + args.clip) * replay_advantage,
                ) * replay_weight).sum() / replay_denominator
                loss = loss + winning_loss
                replay_kl_mean = (replay_kl * replay_weight).sum() / replay_denominator
            backward_started = time.monotonic()
            optimizer.zero_grad(set_to_none=True); loss.backward()
            fresh_cpu = fresh.detach().cpu().numpy()
            fresh_rows = int(fresh_cpu.sum())
            dataset.ratio_dropped += len(fresh_cpu) - fresh_rows
            if fresh_rows:
                if float(kl) > args.target_kl:
                    dataset.kl_dropped += fresh_rows
                else:
                    nn.utils.clip_grad_norm_(model.parameters(), .5); optimizer.step()
                    clip_fraction = (((ratio - 1).abs() > args.clip).to(mask.dtype) * mask).sum().div(denominator).detach()
                    losses["mean_advantage"].append(float(np.mean(
                        values["advantage"][fresh_cpu] + progress_beta * values["progress_advantage"][fresh_cpu]
                    )))
                    losses["policy_loss"].append(policy_loss.detach())
                    losses["value_loss"].append(value_loss.detach())
                    losses["progress_value_loss"].append(progress_value_loss.detach())
                    losses["progress_beta"].append(progress_beta)
                    losses["entropy"].append(entropy.detach())
                    losses["kl"].append(kl); losses["clip_fraction"].append(clip_fraction)
                    trained += fresh_rows; updates += 1
                    last_batch = (
                        [row for row, selected in zip(rows, fresh_cpu) if selected],
                        values["action"][fresh_cpu], values["old"][fresh_cpu],
                    )
                    if replay_valid is not None:
                        valid_cpu = replay_valid.cpu().numpy()
                        eligible_cpu = replay_eligible.cpu().numpy()
                        current_policies = replay_distribution.logits.detach().cpu().numpy()
                        replay_characters = [int(np.asarray(sample[0][0])[
                            model.character_start : model.character_start + 5
                        ].argmax()) for sample in replay]
                        for sample, policy, eligible, keep in zip(
                            replay, current_policies, eligible_cpu, valid_cpu
                        ):
                            if eligible and not keep:
                                sample[4][:] = np.asarray(policy[:len(sample[4])], np.float16)
                        winning_rejected += int((eligible_cpu & ~valid_cpu).sum())
                        for character, eligible, keep in zip(
                            replay_characters, eligible_cpu, valid_cpu
                        ):
                            if eligible:
                                (winning_replayed_characters if keep else winning_reanchored_characters)[character] += 1
                        replay_count = int(valid_cpu.sum())
                        if replay_count:
                            behavior = torch.as_tensor([sample[2] for sample in replay], device=target)
                            winning_behavior_drift = (
                                replay_log_probability.detach()[replay_valid] - behavior[replay_valid]
                            ).abs().mean()
                            losses["winning_loss"].append(winning_loss.detach())
                            losses["winning_kl"].append(replay_kl_mean.detach())
                            winning_replayed += replay_count
                    publish()
            backward_seconds = time.monotonic() - backward_started
            update_elapsed = time.monotonic() - update_started
            update_seconds += update_elapsed
            if update_elapsed > 5:
                packed = rows + [sample[0] for sample in replay]
                scores = minibatch_scores(packed, model)
                represented_actions = max(len(row[3]) for row in packed)
                print(json.dumps({"slow_update": {
                    "seconds": update_elapsed, "action_band": fresh_action_band,
                    "fresh": len(rows), "replay": len(replay),
                    "score_min": int(min(scores)), "score_max": int(max(scores)),
                    "state_tokens_max": max(len(row[1]) for row in packed),
                    "represented_actions_max": represented_actions,
                    "state_attention_pairs": sum((len(row[1]) + 1) ** 2 for row in packed),
                    "action_attention_pairs": len(packed) * (represented_actions + 2) ** 2,
                    "legal_actions_max": max(np.count_nonzero(row[7]) for row in packed),
                    "unpack_seconds": unpack_seconds,
                    "forward_seconds": forward_seconds, "backward_seconds": backward_seconds,
                }}), flush=True)
            absolute = base_decisions + handled
            if absolute >= next_save:
                save_step(absolute)
                next_save += args.save_decisions
            if absolute >= next_report:
                windows += 1
                report()
                next_report += args.report_decisions
                recent = [rows[-args.promotion_window:] for rows in episodes]
                promotion_ready |= stage + 1 < len(STAGES) and all(
                    len(rows) >= args.promotion_window
                    and sum(row[0] for row in rows) / len(rows) >= args.promote_win_rate
                    for rows in recent
                )
                if promotion_ready:
                    stop.set()
    finally:
        stop.set()
        packer.shutdown(wait=True, cancel_futures=True)
        executor.shutdown(wait=True, cancel_futures=True)
    ingest(future.result())
    assert handled == decisions and not len(dataset)
    assert sampled == decisions + discarded_steps
    if decisions and (base_decisions + handled) % args.report_decisions:
        windows += 1
        report()
    terminal = [episode for character in episodes for episode in character]
    return {
        "decisions": decisions, "updates": updates, "windows": windows,
        "decisions_per_second": decisions / (time.monotonic() - started),
        "seconds": time.monotonic() - started,
        "collect_seconds": collect_seconds, "update_seconds": update_seconds,
        "stage": {"index": stage, "ascension": ascension, "bonus": bonus},
        "characters": summaries(episodes),
        "terminals": {
            "episodes": len(terminal), "wins": sum(row[0] for row in terminal),
            "floor_bands": bands([row[1] for row in terminal]),
            "caps": sum(row[2] or row[3] or row[4] for row in terminal),
            "step_caps": sum(row[2] for row in terminal),
            "combat_caps": sum(row[3] for row in terminal),
            "empty_actions": sum(row[4] for row in terminal) + orphan_empty_actions,
        },
        "description": f"Continuously trained V{MODEL_VERSION} on A{ascension}/+{bonus} trajectories.",
        "pipeline": pipeline, "promotion_ready": promotion_ready,
        "dataset_rows": len(dataset), "dataset_peak": dataset_peak, "dataset_seen": dataset.seen,
        "dataset_attempted": attempted, "dataset_trained": trained,
        "dataset_stale_dropped": dataset.stale_dropped,
        "dataset_ratio_dropped": dataset.ratio_dropped, "dataset_kl_dropped": dataset.kl_dropped,
        "sampled_decisions": sampled, "discarded_steps": discarded_steps,
        "discarded_step_fraction": discarded_steps / max(1, sampled),
        "completed_trajectories": len(trajectory_lengths),
        "trajectory_length_mean": float(np.mean(trajectory_lengths)) if trajectory_lengths else 0,
        "trajectory_length_max": max(trajectory_lengths, default=0),
        "trajectory_policy_span_mean": float(np.mean(trajectory_policy_spans)) if trajectory_policy_spans else 0,
        "trajectory_policy_span_max": max(trajectory_policy_spans, default=0),
        "trajectory_completion_seconds_mean": float(np.mean(trajectory_seconds)) if trajectory_seconds else 0,
        "trajectory_completion_seconds_max": max(trajectory_seconds, default=0),
        "policy_version": updates, "sampler_version": latest_sampler_version,
        "policy_lag_mean": float(np.mean(policy_lags)) if policy_lags else 0,
        "winning_reservoir": len(reservoir.rows), "winning_seen": reservoir.seen,
        "winning_character_rows": list(map(len, reservoir.by_character(model.character_start))),
        "winning_episodes": reservoir.wins, "winning_added": winning_added,
        "winning_skipped": reservoir.skipped, "winning_replayed": winning_replayed,
        "winning_rejected_kl": winning_rejected,
        "winning_replayed_characters": winning_replayed_characters,
        "winning_reanchored_characters": winning_reanchored_characters,
        "winning_replay_fraction": winning_replayed / max(1, trained + winning_replayed),
        "winning_loss_weight": .1,
        "winning_collection_drift": float(winning_behavior_drift),
        "observed_kl": observed_kl, "observed_clip_fraction": observed_clip,
        **metric_means(losses),
    }


def evaluate(model, args, target, seed=None, runs=None, stage=None, max_steps=None, max_combat_steps=None):
    model.eval()
    ascension, bonus = STAGES[stage] if stage is not None else (args.ascension, args.bonus)
    reports = []
    for character in range(5):
        outcomes = []; floors = []
        total = runs or args.runs
        for start in range(0, total, args.evaluation_batch):
            count = min(args.evaluation_batch, total - start)
            env = sts2_sim.Batch(count, (seed or args.validation_seed) + character * 1_000_000 + start, character, ascension=ascension)
            env.set_training_bonus(bonus)
            active = np.ones(count, bool)
            capped = np.zeros(count, bool)
            empty_actions = np.zeros(count, bool)
            steps = np.zeros(count, np.int32)
            combat_steps = np.zeros(count, np.int32)
            for _ in range(max_steps or args.max_steps):
                if not active.any():
                    break
                observation = env.observe_tokens(active.tolist())
                empty = active & ~np.asarray(observation[6], bool).any(1)
                empty_actions |= empty
                capped |= empty
                active &= ~empty
                if not active.any():
                    break
                choice = act(model, observation, target, False, args.precision)[0]
                in_combat = np.asarray([row[4] == 1 for row in env.stats()])
                combat_steps = np.where(active & in_combat, combat_steps + 1, 0)
                _, done, _ = env.step(choice.tolist(), active.tolist())
                done = np.asarray(done, bool)
                steps += active
                still_combat = np.asarray([row[4] == 1 for row in env.stats()])
                cap = active & ~done & (
                    (steps >= (max_steps or args.max_steps))
                    | ((combat_steps >= (max_combat_steps or args.max_combat_steps)) & still_combat)
                )
                capped |= cap
                active &= ~(done | cap)
            for index, row in enumerate(env.stats()):
                outcomes.append(None if capped[index] else int(row[4] == 12))
                floors.append(int((row[0] - 1) * 17 + row[1]))
        wins = sum(outcome == 1 for outcome in outcomes)
        caps = sum(outcome is None for outcome in outcomes)
        reports.append({
            "character": character, "runs": total, "resolved": total - caps, "wins": wins,
            "win_rate": wins / max(1, total - caps), "floor_mean": float(np.mean(floors)),
            "floor_bands": bands(floors), "outcomes": outcomes, "floors": floors,
            "caps": caps, "empty_actions": int(empty_actions.sum()),
        })
    return {"stage": {"ascension": ascension, "bonus": bonus}, "characters": reports, "wins": sum(row["wins"] for row in reports), "runs": 5 * (runs or args.runs)}


def immutable_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as output:
        json.dump(value, output, indent=2, sort_keys=True)


def atomic_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True))
    temporary.replace(path)


def save_checkpoint(path, model, optimizer, manifest, stage, decisions, sampler_session,
                    promotion_index, progress_active, reservoir, auxiliary_decisions, replace=False):
    if path.exists() and not replace:
        raise FileExistsError(path)
    target = path.with_suffix(path.suffix + ".tmp") if replace else path
    with target.open("wb" if replace else "xb") as output:
        torch.save({
            "schema": 1, "model_version": MODEL_VERSION, "feature_version": FEATURE_VERSION,
            "fingerprint": manifest["fingerprint"], "precision": manifest["precision"],
            "layout": model.layout, "architecture": manifest["architecture"],
            "stage": stage, "decisions": decisions, "sampler_session": sampler_session,
            "promotion_index": promotion_index,
            "progress_active": progress_active,
            "auxiliary_decisions": auxiliary_decisions,
            "winning_reservoir": reservoir.state_dict() if reservoir else None,
            "model": model.state_dict(), "optimizer": optimizer.state_dict(),
            "torch_rng": torch.get_rng_state(),
        }, output)
    if replace:
        target.replace(path)
    return hashlib.sha256(path.read_bytes()).hexdigest()


def migrate_optimizer(state, parameter_count):
    groups = state["param_groups"]
    parameters = [parameter for group in groups for parameter in group["params"]]
    if len(parameters) == parameter_count:
        return True
    if len(groups) != 1 or len(parameters) != parameter_count + 15:
        return False
    embeddings = parameters[2:18]
    rows = [state["state"].get(parameter) for parameter in embeddings]
    if any(rows):
        if not all(rows):
            return False
        state["state"][embeddings[0]] = {
            key: torch.cat([row[key] for row in rows])
            if torch.is_tensor(value) and value.ndim else value
            for key, value in rows[0].items()
        }
    for parameter in embeddings[1:]:
        state["state"].pop(parameter, None)
    groups[0]["params"] = parameters[:3] + parameters[18:]
    return True



def dashboard(target):
    if (target / "run.json").exists():
        target = target.parent
    runs = {}
    for path in target.glob("*/run.json"):
        try:
            manifest = json.loads(path.read_text())
        except (OSError, json.JSONDecodeError):
            continue
        run = path.parent
        reports = {}
        for report in list((run / "reports").glob("*.json")) + [run / "live.json"]:
            try:
                row = json.loads(report.read_text())
                row["_written"] = report.stat().st_mtime
                reports[int(row["step"])] = row
            except (OSError, json.JSONDecodeError, KeyError, ValueError):
                pass
        promotions = {}
        for promotion in (run / "promotions").glob("*.json"):
            try:
                row = json.loads(promotion.read_text())
                row["_written"] = promotion.stat().st_mtime
                promotions[int(row["step"])] = row
            except (OSError, json.JSONDecodeError, KeyError, ValueError):
                pass
        if not reports:
            archived = {}
            for suffix in ("live", "train"):
                for report in (run / "generations").glob(f"*.{suffix}.json"):
                    try:
                        row = json.loads(report.read_text())
                        archived[int(row["generation"])] = row
                    except (OSError, json.JSONDecodeError, KeyError, ValueError):
                        pass
            gate_floors = {}
            for report in (run / "generations").glob("*.gate.json"):
                try:
                    gate = json.loads(report.read_text())
                    result = (gate.get("confirmation") or {}).get("candidate") or gate.get("candidate", {})
                    gate_floors[int(gate["generation"])] = [
                        floor for character in result.get("characters", [])
                        for floor in character.get("floors", [])
                    ]
                except (OSError, json.JSONDecodeError, KeyError, ValueError):
                    pass
            offset = 0
            for row in archived.values():
                history = row.get("history", [])
                for index, point in enumerate(history):
                    step = offset + int(point.get("steps", 0))
                    metrics = dict(point)
                    if index + 1 == len(history) and gate_floors.get(int(row["generation"])):
                        metrics["terminal_floors"] = gate_floors[int(row["generation"])]
                    reports[step] = {
                        "step": step, "stage": row.get("stage", {}),
                        "description": row.get("description", "Archived training report.").replace("Generation", "Archived cycle").replace("generation", "archived cycle"),
                        "pipeline": [value.replace("generation", "cycle") for value in row.get("pipeline", [])],
                        "metrics": metrics,
                    }
                offset += max((int(point.get("steps", 0)) for point in history), default=0)
        if not reports:
            reports[0] = {"step": 0, "description": "No training reports yet.", "pipeline": [], "metrics": {}}
        name = str(run.relative_to(target))
        try:
            best = json.loads((run / "best.json").read_text())
        except (OSError, json.JSONDecodeError):
            best = None
        runs[name] = {
            "version": manifest.get("model_version", manifest.get("version", 0)),
            "manifest": manifest, "source": manifest.get("source"),
            "reports": [
                {"iteration": reports[key]["metrics"].get("iteration", index),
                 "step": reports[key]["step"], "stage": reports[key].get("stage", {}),
                 "description": reports[key].get("description", ""),
                 "pipeline": reports[key].get("pipeline", []),
                 "metrics": reports[key]["metrics"]}
                for index, key in enumerate(sorted(reports), 1)
            ],
            "promotions": [promotions[key] for key in sorted(promotions)],
            "best": best,
        }
    def history(name, seen=()):
        row = runs[name]
        source = Path(row["source"]).resolve() if row["source"] else None
        parents = [candidate for candidate in runs if candidate not in seen + (name,)
                   and source and source.is_relative_to((target / candidate).resolve())]
        parent = max(parents, key=len, default=None)
        inherited = history(parent, seen + (name,)) if parent and runs[parent]["version"] == row["version"] and runs[parent]["manifest"].get("architecture") == row["manifest"].get("architecture") else []
        return list({report["step"]: report for report in inherited + row["reports"]}.values())
    histories = {name: sorted(history(name), key=lambda report: report["step"]) for name in runs}
    for name in runs:
        runs[name]["reports"] = histories[name]
        runs[name].pop("source")
    versions = {}
    for name, run in runs.items():
        key = str(run["version"])
        if key not in versions or run["reports"][-1]["step"] > versions[key]["reports"][-1]["step"]:
            versions[key] = run | {"run": name}
    versions = dict(sorted(versions.items(), key=lambda item: int(item[0])))
    data = json.dumps(versions, separators=(",", ":")).replace("</", "<\\/")
    content = """<!doctype html><meta charset=utf-8><title>Spirefysh dashboard</title><script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script><style>
body{font:14px system-ui;margin:24px;background:#101319;color:#e8ecf2}h1,h2{margin-bottom:6px}.controls,.pipeline{display:flex;align-items:center;gap:12px;flex-wrap:wrap;margin-bottom:22px}select,input{padding:7px;background:#202938;color:#e8ecf2;border:1px solid #526176;border-radius:5px}.charts{display:grid;grid-template-columns:repeat(auto-fit,minmax(440px,1fr));gap:14px}.panel{margin:22px 0;padding:16px;background:#171d28;border:1px solid #303a49;border-radius:10px}.charts .panel{margin:0}.plot{height:340px;min-width:0}.pipeline span{padding:10px;background:#202938;border-radius:6px}.pipeline span+span:before{content:'→';margin-right:12px;color:#8794a8}table{border-collapse:collapse;width:100%}th,td{padding:7px;border-bottom:1px solid #303a49;text-align:left}.yes{color:#75db91}.no{color:#ee7b7b}.state-flow{display:grid;grid-template-columns:1fr auto 1fr auto 1fr auto 1.2fr;gap:10px;align-items:stretch;margin:14px 0}.state-flow.action-flow{grid-template-columns:1fr auto 1fr auto 1fr}.state-node{padding:14px;border:1px solid #526176;border-radius:8px;background:#202938}.state-node b{display:block;font-size:18px;margin-bottom:5px}.state-node[data-tip]{cursor:help}.module-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(210px,1fr));gap:10px;margin:14px 0}.module-grid .state-node{min-height:68px}.state-plus{align-self:center;color:#8794a8;font-size:24px}.state-inspectors,.latent-columns{display:grid;grid-template-columns:repeat(auto-fit,minmax(440px,1fr));gap:14px}.state-box{padding:14px;background:#121824;border:1px solid #303a49;border-radius:8px}.state-box summary{cursor:pointer;font-weight:700;margin-bottom:12px}.training-diagrams{display:grid;gap:14px}.training-diagram{padding:14px;background:#121824;border:1px solid #303a49;border-radius:8px}.training-diagram h3{margin:0 0 10px}.flow-row{display:flex;align-items:stretch;gap:8px;overflow-x:auto;padding:3px}.flow-node{flex:1;min-width:165px;padding:12px;border:1px solid #526176;border-top:3px solid var(--cell);border-radius:7px;background:#202938;cursor:help}.flow-node b{display:block;margin-bottom:5px}.flow-node small{display:block;color:#aeb8c8;line-height:1.35}.flow-arrow{align-self:center;color:#8794a8;font-size:22px}.float-group{margin:12px 0}.float-group h4,.latent-row h4{margin:4px 0;color:#b9c7d8}.float-grid{display:grid;grid-template-columns:repeat(32,11px);gap:2px}.float-grid.token{grid-template-columns:repeat(8,42px);gap:5px}.float-grid.latent{grid-template-columns:repeat(64,7px);gap:2px}.float{width:11px;height:11px;padding:0;border:1px solid color-mix(in srgb,var(--cell) 75%,#fff 10%);border-radius:2px;background:color-mix(in srgb,var(--cell) 45%,#101319);cursor:help}.float.token{width:42px;height:28px;color:#e8ecf2;font-size:10px}.float.latent{width:7px;height:7px}.float:hover,.float:focus,.state-node[data-tip]:hover,.state-node[data-tip]:focus,.flow-node:hover,.flow-node:focus{outline:2px solid #fff;position:relative;z-index:2}.latent-row{margin:10px 0}.state-tip{position:fixed;display:none;z-index:1000;max-width:520px;padding:10px 12px;border:1px solid #6fb1ff;border-radius:7px;background:#080b10;color:#e8ecf2;white-space:pre-wrap;box-shadow:0 8px 28px #000b;pointer-events:none}.state-note{color:#aeb8c8}.unsupported{padding:18px;border:1px dashed #526176;border-radius:8px;color:#aeb8c8}
</style><h1 id=title>Spirefysh dashboard</h1><div class=controls><label>Model version <select id=version></select></label><label>X axis <select id=xaxis><option value=iteration>Iteration</option><option value=decisions selected># decisions</option><option value=time>Wall-clock time</option></select></label><label><input id=smooth type=checkbox checked> EMA</label><label>EMA α <input id=ema type=number min=.01 max=1 step=.01 value=.2></label></div><section class=panel><h2>Selected model</h2><div id=details></div></section><div class=charts><section class=panel><h2>Mean advantage</h2><div id=advantage class=plot></div></section><section class=panel><h2>Decisions / second</h2><div id=throughput class=plot></div></section><section class=panel><h2>Terminal floor</h2><div id=floor class=plot></div></section><section class=panel><h2>Ascension</h2><div id=ascension class=plot></div></section><section class=panel><h2>Bonus strength</h2><div id=bonus class=plot></div></section><section class=panel><h2>Win proportion</h2><div id=wins class=plot></div></section><section class=panel><h2>Clip fraction</h2><div id=clip class=plot></div></section><section class=panel><h2>KL</h2><div id=kl class=plot></div></section><section class=panel><h2>Entropy</h2><div id=entropy class=plot></div></section></div><section class=panel><h2>Promotion</h2><div id=promotion></div></section><section class=panel><h2>State representation</h2><p class=state-note>Hover or focus any square to see the exact source of that float.</p><div id=stateDiagram></div></section><section class=panel><h2>Training pipeline</h2><p class=state-note>Hover or focus any stage for its exact rule, limit, and latest measured value.</p><div id=trainingDiagram></div></section><div id=stateTip class=state-tip></div><script>const versions=""" + data + r""",versionSelect=document.querySelector('#version'),xaxis=document.querySelector('#xaxis'),smooth=document.querySelector('#smooth'),ema=document.querySelector('#ema');
const characterNames=['Ironclad','Defect','Silent','Regent','Necrobinder'],characterColors=['#ef4444','#38bdf8','#22c55e','#f59e0b','#a78bfa'],names=Object.keys(versions),config={responsive:true,displaylogo:false};versionSelect.innerHTML=names.map(name=>`<option value="${name}">V${name}</option>`).join('');versionSelect.value=names.at(-1)||'';
function x(report){return xaxis.value==='time'?(report.metrics.seconds||0)/60:xaxis.value==='decisions'?report.step:report.iteration}function series(history,key){return history.map(report=>({x:x(report),y:Number(report.metrics[key])})).filter(point=>Number.isFinite(point.y))}
function emaLine(points){const alpha=Math.max(.01,Math.min(1,Number(ema.value)||.2));let value;return points.map((point,index)=>({x:point.x,y:value=index?alpha*point.y+(1-alpha)*value:point.y}))}
function stageTransitions(history,run){const promotions=(run?.promotions||[]).filter(row=>row.promoted);if(promotions.length)return promotions.map(promotion=>{const report=history.find(row=>row.step===promotion.step)||history.filter(row=>row.step<=promotion.step).at(-1),next=run.manifest.stages?.[(promotion.stage?.index??-1)+1]||history.find(row=>row.step>promotion.step)?.stage;let position=xaxis.value==='decisions'?promotion.step:report&&xaxis.value==='iteration'?report.metrics.iteration:report?(Number.isFinite(Number(promotion.seconds))?promotion.seconds:report.metrics.seconds+Math.max(0,(promotion._written??report._written)-(report._written??promotion._written)))/60:NaN;return{x:position,stage:next}}).filter(point=>Number.isFinite(Number(point.x)));return history.slice(1).flatMap((row,index)=>Number.isFinite(Number(row.stage?.ascension))&&Number.isFinite(Number(history[index].stage?.ascension))&&(row.stage.ascension!==history[index].stage.ascension||row.stage.bonus!==history[index].stage.bonus)?[{x:x(row),stage:row.stage}]:[])}
function stageLines(history,run){return stageTransitions(history,run).map(point=>({type:'line',xref:'x',yref:'paper',x0:point.x,x1:point.x,y0:0,y1:1,layer:'below',line:{color:'rgba(232,236,242,.38)',width:1,dash:'dash'}}))}
function layout(percent=false,range,history=[],run){const title=xaxis.value==='time'?'Wall-clock time (minutes)':xaxis.value==='decisions'?'# decisions':'Iteration';return{template:'plotly_dark',paper_bgcolor:'rgba(0,0,0,0)',plot_bgcolor:'rgba(0,0,0,0)',margin:{l:62,r:18,t:12,b:52},hovermode:'closest',showlegend:true,shapes:stageLines(history,run),xaxis:{title,gridcolor:'#303a49'},yaxis:{gridcolor:'#303a49',tickformat:percent?'.0%':undefined,range,zerolinecolor:'#8794a8'}}}
function plot(id,points,{range,percent=false,tozero=false,history=[],run}={}){const traces=[{x:points.map(point=>point.x),y:points.map(point=>point.y),mode:'lines+markers',name:'raw',line:{color:'#6fb1ff',width:2},marker:{color:'#6fb1ff',size:6,opacity:.8},hovertemplate:'x %{x}<br>y %{y:.5g}<extra></extra>'}];if(smooth.checked&&points.length>1){const line=emaLine(points);traces.push({x:line.map(point=>point.x),y:line.map(point=>point.y),mode:'lines',name:`EMA α=${Number(ema.value)||.2}`,line:{color:'#ffb454',width:4},hovertemplate:'EMA %{y:.5g}<extra></extra>'})}const options=layout(percent,range,history,run);if(tozero)options.yaxis.rangemode='tozero';Plotly.react(id,traces,options,config)}
function floorPlot(history,run){const training=run.manifest.sessions?.at(-1)?.training||run.manifest.training||{},envs=training.envs||1,points=[];for(const report of history){let inferred=0,last=-Infinity;for(const row of report.metrics.trajectory_floors||[]){const [iteration,floor,stored]=row;if(stored===undefined&&iteration<last)inferred++;const character=stored??Math.min(inferred,4);points.push({x:xaxis.value==='iteration'?iteration:xaxis.value==='decisions'?iteration*envs:(report.metrics.seconds||0)/60,y:floor,character,iteration,step:report.step});last=iteration}}for(let i=points.length-1;i>0;i--){const j=Math.floor(Math.random()*(i+1));[points[i],points[j]]=[points[j],points[i]]}const trajectory={x:points.map(point=>point.x),y:points.map(point=>point.y),customdata:points.map(point=>[characterNames[point.character],point.iteration,point.step]),mode:'markers',showlegend:false,marker:{color:points.map(point=>characterColors[point.character]),size:6,opacity:.5},hovertemplate:'%{customdata[0]}<br>floor %{y}<br>iteration %{customdata[1]:,}<br>report decisions %{customdata[2]:,}<extra></extra>'},legend=characterNames.map((name,character)=>({x:[null],y:[null],mode:'markers',name,hoverinfo:'skip',marker:{color:characterColors[character],size:7,opacity:.5}})),options=layout(false,[0,52],history,run);options.yaxis.title='Terminal floor';Plotly.react('floor',[trajectory,...legend],options,config)}
function stagePlot(id,history,key,color,run){const rows=history.filter(row=>Number.isFinite(Number(row.stage?.[key]))).map(row=>({x:x(row),y:row.stage[key]}));for(const transition of stageTransitions(history,run))if(Number.isFinite(Number(transition.stage?.[key])))rows.push({x:transition.x,y:transition.stage[key]});rows.sort((left,right)=>left.x-right.x);const options=layout(false,undefined,history,run);options.yaxis={...options.yaxis,title:key==='ascension'?'Ascension':'Bonus strength',rangemode:'tozero',dtick:key==='ascension'?1:4};Plotly.react(id,[{x:rows.map(row=>row.x),y:rows.map(row=>row.y),mode:'lines+markers',name:key==='ascension'?'Ascension':'Bonus strength',line:{color,width:3,shape:'hv'},marker:{color,size:6},hovertemplate:`${key==='ascension'?'ascension':'bonus'} %{y}<extra></extra>`}],options,config)}
const scalarBase=[
['Run HP fraction','max(run.hp, 0) / max(run.max_hp, 1)'],['Run maximum HP','run.max_hp / 100'],['Gold','run.gold / 500'],['Run act','run.act / 3'],['Act floor','run.floor / 18'],['Run energy','run.energy / 10'],['Run draw','run.draw / 10'],['Run orb slots','run.orb_slots / 10'],['Shop removals bought','run.card_shop_removals / 10'],['Ascension','run.ascension / 10'],
['Happy Flower counter','game.happy_flower / 3'],['Tea Set active','float(game.tea_set)'],['Lasting Candy counter','game.lasting_candy / 3'],["Pael's Wing counter","game.paels_wing / 3"],['Silver Crucible counter','game.silver_crucible / 10'],['Silver Treasures counter','game.silver_treasures / 10'],['Winged Boots charges','game.winged_boots / 3'],['Rest uses','game.rest_used / 255'],['Girya uses','game.girya / 3'],['Pumpkin Candle counter','game.pumpkin_candle / 10'],
['Toy Box combats','game.toy_box_combats / 10'],['Nunchaku counter','game.nunchaku / 10'],['Pendulum counter','game.pendulum / 10'],['Pen Nib counter','game.pen_nib / 10'],['Iron Club counter','game.iron_club / 10'],['Joss Paper counter','game.joss_paper / 10'],['Tuning Fork counter','game.tuning_fork / 10'],['Galactic Dust counter','game.galactic_dust / 10'],['Book of Five Rings counter','game.book_of_five_rings / 10'],['Ember Tea counter','game.ember_tea / 10'],['Sword of Stone counter','game.sword_of_stone / 10'],
['Damage-taken flag','float(game.damage_taken)'],['Maw Bank active','float(game.maw_bank)'],['Lizard Tail available','float(game.lizard_tail)'],['Cooking active','float(game.cooking)'],['Card rarity offset','game.rarity_offset / 10'],['Potion probability','game.potion_odds / 100'],...Array.from({length:4},(_,i)=>[`Unknown-event probability ${i}`,`game.unknown_odds[${i}] / 100`]),['Bosses visited','game.bosses_visited / 2'],['Event-combat counter','game.event_combat / 10'],['Fake Happy Flower counter','game.fake_happy_flower / 3'],['Pollinous Core counter','game.pollinous_core / 10'],['Silken Tress active','float(game.silken_tress)'],['Bone Tea active','float(game.bone_tea)'],['Tea of Discourtesy active','float(game.tea_of_discourtesy)'],['Replacing potion','float(game.replacing_potion)'],['Cards rerolled','float(game.rerolled_cards)'],['Removal price','game.removal_price / 500'],['Weak encounters remaining','game.weak_encounters_left / 15'],['Regular encounters remaining','game.regular_encounters_left / 15'],['Elite encounters remaining','game.elite_encounters_left / 15'],['Fishing Rod charges','game.fishing_rod / 3'],['Crystal turns remaining','crystal.remaining / 10; zero without a crystal'],['Large crystal flag','float(crystal.big); zero without a crystal'],['Cleared crystal cells','count(crystal.clear) / 25; zero without a crystal'],['Revealed crystal items','crystal.revealed.len / 25; zero without a crystal']];
const combatScalar=[
['Phase slot 0 / combat energy','Combat: combat.energy / 10; Rewards: rewards.gold / 500; Shop: removal price / 500; Remove/Upgrade/Choose: count / 10; Transform: (target_id + 1) / 1000; Enchant: amount / 100'],['Phase slot 1 / maximum energy','Combat: combat.max_energy / 10; Rewards: removals / 10; Remove: price / 500; Upgrade: optional flag; Transform/Enchant: count / 10; Choose: reward flag'],['Phase slot 2 / draw per turn','Combat: combat.draw_per_turn / 10; Remove/Transform: optional flag; Enchant: card type / 10'],['Phase slot 3 / stars','Combat: combat.stars / 10; Enchant: optional flag'],['Combat turn','combat.turn / 20'],['Combat orb slots','combat.orb_slots / 10'],['Player combat HP fraction','max(combat.player.hp, 0) / max(combat.player.max_hp, 1)'],['Player block','combat.player.block / 100'],['Osty HP fraction','max(combat.osty.hp, 0) / max(combat.osty.max_hp, 1)'],['Osty block','combat.osty.block / 100'],
...['cards','manual_cards','manual_plays','attacks','skills','powers','energy','exhausted','discarded','shivs','stars_gained','generated','ethereal','extra_drawn','doom_applied','osty_attacks','block_gains','hp_lost','hp_loss_events','feral_returns'].map(name=>[`Combat history: ${name}`,`combat.history.${name} / 20`]),
...['block_card','block_card_gains'].map(name=>[`Combat history: ${name}`,`combat.history.${name} / 20`]),...['last_cards','orbit_spent','last_damage','drawn','lightning_channeled','orbs_channeled','poisoned','card_energy','card_stars','card_plays','enemy_turn','ending','force_end','paels_tears','centennial_puzzle','demon_tongue','permafrost','pen_nib','ruined_helmet','music_box','mini_regent','rainbow_ring','kusarigama','unsettling_used','diamond_diadem'].map(name=>[`Combat state: ${name}`,`combat.${name} / 20`])];
const cardNumeric=[['Upgrades','card.upgrades'],['Enchantment amount','card.enchantment_amount'],['Enchantment value','card.enchantment_value'],['Variant','card.variant'],['Persistent flags','card.flags'],['Turn flags','card.turn_flags'],['Card value','card.value'],['Replay count','card.replays'],['Free flag','float(card.free)'],['Printed energy cost','card definition cost at upgrade'],['Printed star cost','card definition star cost at upgrade'],['Cost delta','card.cost_delta'],['Cost override','0 or card.cost_override + 129'],['Master-card relation','master instance/index relation to run deck'],['Related upgrades','upgrades on related master card'],['Context','zone-specific price, bundle or delayed context'],['Effective energy cost','current computed energy cost after effects'],['Effective star cost','current computed star cost'],['Preview block','deterministic card preview block'],['Chosen-target damage','sum preview damage for chosen/random/lowest targets'],['All-target damage','sum preview damage for all/other targets'],['Preview draw','preview.draw'],['Preview discard','preview.discard'],['Preview exhaust','preview.exhaust']];
const legacyCardZones=['Run deck','Combat hand','Draw pile','Discard pile','Exhaust pile','Combat offer pile','Reward/shop/choice/bundle offers','History Course card',"Pael's stored cards",'Slippery Bridge selected card','Slippery Bridge candidates','Currently playing card','Queued automatic plays'],cardZones=['Run deck','Combat hand','Draw pile','Discard pile','Exhaust pile','History Course card',"Pael's stored cards",'Slippery Bridge previous cards','Currently playing card','Queued automatic plays'];
const entityNames=['Enemies','Powers','Delayed effects','Relics','Potions','Continuations','Map','Crystal board','Encounters'];
function globalFields(run){const fields=[],add=(group,name,source)=>fields.push({group,name,source});scalarBase.forEach(([name,source])=>add('Scalar bank',name,source));combatScalar.forEach(([name,source])=>add('Scalar bank',name,source));add('Scalar bank','Reserved scalar 116','Currently zero; reserved phase-local scalar slot');add('Scalar bank','Reserved scalar 117','Currently zero; reserved phase-local scalar slot');for(let i=0;i<10;i++)add('Scalar bank',`Reward/Tinker summary ${i}`,i<8?`Rewards: card reward ${Math.floor(i/2)} ${i%2?'detail':'kind'}; Tinker Time: ${i?'offer '+i+' present':'event_data[3] / 3'}`:`Rewards: zero; Tinker Time: offer ${i} present`);for(let i=0;i<5;i++)add('Character one-hot',`Character ${i}`,`1 when run.character == ${i}, else 0`);for(let i=0;i<5;i++)add('Act one-hot',`Act ${i}`,`1 when game.act == ${i}, else 0`);for(let i=0;i<14;i++)add('Phase one-hot',`Phase ${i}`,`1 when phase_index(game.phase) == ${i}, else 0`);for(let i=0;i<8;i++)add('Room one-hot',`Room ${i}`,`1 when room_index(game.room) == ${i}, else 0`);for(let i=0;i<66;i++)add('Current event',`Event ID ${i}`,`1 when current phase is Event(${i}), else 0`);for(let i=0;i<66;i++)add('Visited events',`Visited event ID ${i}`,`1 when game.visited_events contains ${i}, else 0`);for(let i=0;i<4;i++)add('Event requirements',`Requirement ${i}`,`accumulated event option requirement feature ${i}`);for(let i=0;i<36;i++)add('Event effects',`Effect ${i}`,`accumulated event option effect feature ${i}`);for(let i=0;i<8;i++)add('Event values',`Event value ${i}`,`normalized event option numeric value ${i}`);for(let i=0;i<4;i++)add('Event-local state',`Event state ${i}`,`game.event_data[${i}] / 1000 for supported events, else 0`);for(let i=0;i<22;i++)add('Enchant choice',`Enchantment ${i}`,`1 when EnchantCards selects enchantment ${i}, else 0`);const globals=run.manifest.layout.globals,orbSlots=(globals-400)/6;if(!Number.isInteger(orbSlots)||orbSlots<0)return[];for(let slot=0;slot<orbSlots;slot++){for(let id=0;id<5;id++)add('Orb slots',`Orb slot ${slot}: type ${id}`,`1 when combat.orbs[${slot}].id == ${id}, else 0`);add('Orb slots',`Orb slot ${slot}: value`,`combat.orbs[${slot}].value / 100; zero when slot is empty`)}[['Osty maximum HP','combat.osty.max_hp / 500'],['Burning Sticks','float(combat.burning_sticks)'],['Throwing Axe','float(combat.throwing_axe)'],["Pael's Eye","float(combat.paels_eye)"],["Pael's Eye extra","float(combat.paels_eye_extra)"],["Pael's Legion",'combat.paels_legion / 2'],['Known draw top','combat.known_draw_top / 255'],['Known draw bottom','combat.known_draw_bottom / 255'],['Nightmare count','combat.nightmares.len / 16'],['Bomb count','combat.bombs.len / 16'],['Automation count','combat.automation.len / 16'],['Panache count','combat.panache.len / 16'],['Boulder count','combat.boulders.len / 16'],['Dampened count','combat.dampened.len / 16']].forEach(([name,source])=>add('Combat summary',name,source));[['Pending curse','float(game.pending_curse)'],['Wongo active','float(game.wongo_combats.is_some())'],['Wongo combats','game.wongo_combats.unwrap_or(0) / 5'],['Golden Compass current act','float(game.golden_compass == run.act)'],['Astrolabe','float(game.astrolabe)'],['Transform niche','float(game.transform_niche)'],["Pael's Tooth","float(game.paels_tooth)"],['Parasol removal','float(game.parasol_removal)'],['Conveyor','float(game.conveyor)'],['Fake shop','float(game.fake_shop)']].forEach(([name,source])=>add('Run-state flags',name,source));[['Choice pile','(pile_index(choice.pile) + 1) / 5'],['Choice filter','filter_features(choice.filter).kind / 15'],['Choice filter value','filter_features(choice.filter).value / 1,000,000'],['Choice operation','op_features(choice.op).kind / 17'],['Choice operation value','op_features(choice.op).value / 1,000,000'],['Choices remaining','choice.remaining / 10'],['Choice/combat flags','bit-pack optional, enemy_turn, ending and force_end; divide by 15'],['Resume phase','(phase_index(game.resume) + 1) / 14; zero without resume']].forEach(([name,source])=>add('Decision context',name,source));add('Training bonuses','Strength bonus','training_strength / 32');add('Training bonuses','Dexterity bonus','training_dexterity / 32');return fields}
function tokenFields(mode){const unused=()=>Array.from({length:24},(_,i)=>[`Numeric ${i}`,'Unused by this token kind; emitted as 0']),spec={card:[10,'State card zone: 1…13 through V36; 1…10 from V37','card.id + 1','1 for state cards','known draw position or 0','card type + 1','enchantment + 1','1 Status, 2 Curse, 3 Quest, else 0',cardNumeric],enemy:[1,'0 live enemy, 1 move history, 2 public Wriggler parity','enemy ID + 1 or prior move + 1','enemy slot for history tokens','enemy slot or history position','0','0','0',unused()],power:[2,'0 current, 1 snapshot','power.id + 1','1 player, 2 Osty, 3+ enemy slot','power list position + 1','0','0','0',unused()],delayed:[3,'17 Nightmare card, 18 dampened card, 2 bomb, 3 automation, 4 panache, 5 boulder, 6 lamp','card/item ID where applicable','card zone/owner where applicable','item position + 1','card type for card variants','enchantment for card variants','affliction for card variants',cardNumeric.map(([n,s])=>[n,`Semantic-card variants: ${s}; otherwise kind-specific or 0`])],relic:[4,'0 owned; 1/2 bags; 3 reward; 4 Parasol; 5 fake merchant; 6 reward; 7 shop; 8–10 trades; 16 ancient offer','relic.id + 1','rarity or event-option owner','inventory/offer position + 1','0','0','0',unused()],potion:[5,'0 slot; 1 pending; 2 reward; 3 shop; 4 Parasol','potion.id + 1, or 0 for empty slot','0','slot/offer position + 1','0','0','0',unused()],continuation:[6,'0 size; 1 serialized chunk; 2 removal; 3 reward gold; 6 conveyor; 19 queued card; 20 resume card; 6 Parasol card','serialized/card ID where applicable','high position bits, phase owner or item position','low position bits or item position','card type for card variants','enchantment for card variants','affliction for card variants',cardNumeric.map(([n,s])=>[n,`Semantic-card variants: ${s}; otherwise continuation-kind-specific or 0`])],map:[7,'0 node, 1 directed edge','room + 1 for node; destination lane + 1 for edge','node floor','node lane + 1','0','0','0',unused()],crystal:[8,'0 crystal cell','visible item + 1 or 0','row + 1','column + 1','0','0','0',unused()],encounter:[9,'0 boss; 1 normal pool; 2 elite pool; 3 last normal; 4 last elite','index + 1 in the corresponding encounter vocabulary','0','0','0','0','0',unused()] }[mode];const [collection,kind,id,owner,position,type,enchantment,affliction,numeric]=spec;if(mode==='enemy'){numeric[0]=['Wriggler parity','(enemy.instance - 1) % 2 + 1 for Wriggler, else 0'];numeric[2]=['Enemy HP','enemy.hp / 500'];numeric[3]=['Enemy maximum HP','enemy.max_hp / 500'];numeric[4]=['Enemy block','enemy.block / 100'];numeric[5]=['Current move','enemy.move_index / 20'];numeric[6]=['Previous move','enemy.last_move / 20, or -1'];numeric[7]=['Repeat count','enemy.repeats / 10'];numeric[8]=['Stunned','float(enemy.stunned)'];numeric[9]=['Enemy value','enemy.value / 100'];numeric[10]=['Predicted hit','combat.hits[slot] / 20']}if(mode==='power'){numeric[0]=['Power amount','power.amount / 32768'];numeric[1]=['Power value','power.value / 32768'];numeric[2]=['Skip duration','float(power.skip_duration)']}if(mode==='delayed'){numeric[0]=['Primary value','Card upgrades; bomb turns/3; automation turns/10; panache cards/5; boulder amount/100'];numeric[1]=['Secondary value','Card enchantment amount; bomb amount/100; automation energy/10; panache damage/20'];numeric[2]=['Tertiary value','Card enchantment value; panache start/20']}if(mode==='relic'){numeric[0]=['Relic state / price','Owned: melted flag; Parasol/shop: price / 500'];numeric[1]=['Wax priority','Owned relic wax priority, else 0']}if(mode==='potion')numeric[0]=['Potion price','Parasol/shop price / 500, else 0'];if(mode==='continuation'){numeric[0]=['Primary continuation value','Card upgrades; byte length low 16 bits; removal price/500; reward gold/500; conveyor value/1000'];numeric[1]=['Secondary continuation value','Card enchantment amount or byte length high bits']}if(mode==='map'){numeric[0]=['Current node','1 when this node is current'];numeric[1]=['Fur Coat node','1 when marked by Fur Coat in current act'];numeric[2]=['Spoils node','1 when marked for spoils']}if(mode==='crystal')numeric[0]=['Cleared cell','float(clear)'];return[['Collection',`constant ${collection}: ${mode}`],['Kind / zone',kind],['ID',id],['Owner',owner],['Position',position],['Card type',type],['Enchantment',enchantment],['Affliction',affliction],...numeric]}
const stateColors={'Scalar bank':'#38bdf8','Character one-hot':'#60a5fa','Act one-hot':'#818cf8','Phase one-hot':'#a78bfa','Room one-hot':'#c084fc','Current event':'#e879f9','Visited events':'#f472b6','Event requirements':'#fb7185','Event effects':'#f97316','Event values':'#f59e0b','Event-local state':'#eab308','Enchant choice':'#84cc16','Orb slots':'#22c55e','Combat summary':'#14b8a6','Run-state flags':'#06b6d4','Decision context':'#0ea5e9','Training bonuses':'#f8fafc'};
const escapeAttribute=value=>String(value).replaceAll('&','&amp;').replaceAll('"','&quot;').replaceAll('<','&lt;').replaceAll('>','&gt;');
function stateCell(index,tip,color,kind=''){return `<button class="float ${kind}" style="--cell:${color}" data-tip="${escapeAttribute(tip)}" aria-label="${escapeAttribute(tip)}" tabindex="0">${kind==='token'?index:''}</button>`}
function renderToken(mode){const fields=tokenFields(mode);document.querySelector('#tokenGrid').innerHTML=fields.map(([name,source],index)=>stateCell(index,`token[${index}] · ${name}\nSource: ${source}`,'#a78bfa','token')).join('')}
function latentRows(start,names,color,kind,width){return names.map((name,group)=>`<div class=latent-row><h4>${name} · state[${start+group*width}…${start+(group+1)*width-1}]</h4><div class="float-grid latent">${Array.from({length:width},(_,latent)=>stateCell(latent,`state[${start+group*width+latent}]\n${name} · latent dimension ${latent}\nSource: ${kind}. This is a learned mixture, not a one-to-one raw feature.`,color,'latent')).join('')}</div></div>`).join('')}
function renderState(run){const target=document.querySelector('#stateDiagram'),manifest=run.manifest,feature=manifest.feature_version,globals=manifest.layout?.globals,width=manifest.architecture?.width||128,zones=manifest.architecture?.card_zones||13;if(feature<33||![460,592].includes(globals)||width!==128||![10,13].includes(zones)){target.innerHTML=`<div class=unsupported>Detailed float provenance is unavailable for feature ${feature}, globals ${globals}, width ${width}, zones ${zones}.</div>`;return}const fields=globalFields(run);if(fields.length!==globals){target.innerHTML=`<div class=unsupported>State metadata mismatch: expected ${globals} global floats, mapped ${fields.length}.</div>`;return}const grouped=new Map;fields.forEach((field,index)=>{if(!grouped.has(field.group))grouped.set(field.group,[]);grouped.get(field.group).push([field,index])});const layout=manifest.layout,layers=manifest.architecture.layers,actionLayers=manifest.architecture.action_layers||0,feedforward=manifest.architecture.feedforward,collections=layout.collections-2,transformer=4*width*width+2*width*feedforward+9*width+feedforward,rows=[layout.collections,layout.kinds,layout.ids,layout.owners,layout.positions,layout.card_types,layout.enchantments,layout.afflictions].reduce((sum,size)=>sum+Math.min(size,257)+Math.max(2,Math.ceil((size-1)/256)+1),0),shared=rows*width+layout.numeric*width+2*width,card=zones*width+layers*transformer,entity=(collections+1)*width,cardStart=globals,entityStart=globals+zones*width,baseState=entityStart+(collections-1)*width,decisionState=baseState+(actionLayers?width:0),action=(layout.action_values+3)*width,actionSet=actionLayers?width+actionLayers*transformer:0,policy=(decisionState+width)*width+2*width+1,value=decisionState*width+2*width+1,total=shared+card+entity+action+actionSet+policy+2*value,number=value=>value.toLocaleString(),tip=(title,formula)=>escapeAttribute(`${title}\n${formula}`),zoneNames=zones===13?legacyCardZones:cardZones,globalHtml=[...grouped].map(([group,rows])=>`<div class=float-group><h4>${group} · ${rows.length}</h4><div class=float-grid>${rows.map(([field,index])=>stateCell(index,`state[${index}] = global[${index}]\n${field.group} · ${field.name}\nSource: ${field.source}`,stateColors[group])).join('')}</div></div>`).join('');target.innerHTML=`<div class=state-flow><div class=state-node data-tip="${tip('Direct global vector',`${globals} raw values; no learned parameters`)}" tabindex=0><b>${globals}</b>direct global floats · 0 params</div><div class=state-plus>+</div><div class=state-node data-tip="${tip('Card-zone state',`${zones} zones × ${width}; learned by the shared token encoder and card Transformer`)}" tabindex=0><b>${zones} × ${width}</b>card-zone outputs</div><div class=state-plus>+</div><div class=state-node data-tip="${tip('Non-card state',`${collections-1} collections × ${width}; learned by shared token encoder and entity pooling`)}" tabindex=0><b>${collections-1} × ${width}</b>entity-pool outputs</div><div class=state-plus>=</div><div class=state-node data-tip="${tip('Base state vector',`${globals} + ${zones*width} + ${(collections-1)*width} = ${baseState} floats`)}" tabindex=0><b>${number(baseState)}</b>${actionLayers?'base':'final'} state floats</div></div>${actionLayers?`<div class="state-flow action-flow"><div class=state-node data-tip="${tip('Base state',`${baseState} floats before the represented action set is summarized`)}" tabindex=0><b>${number(baseState)}</b>base state</div><div class=state-plus>+</div><div class=state-node data-tip="${tip('Action-set context',`learned ${width}-d CLS output from ${actionLayers} Transformer layer over all represented actions`)}" tabindex=0><b>${width}</b>action-set context</div><div class=state-plus>=</div><div class=state-node data-tip="${tip('Decision state',`${baseState} + ${width} = ${decisionState} floats supplied to both value heads and the policy`)}" tabindex=0><b>${number(decisionState)}</b>decision-state floats</div></div>`:''}<div class=module-grid><div class=state-node data-tip="${tip('Shared token encoder',`${rows} embedding rows × ${width} + ${layout.numeric}×${width} numeric projection + ${2*width} LayerNorm = ${number(shared)}`)}" tabindex=0><b>${number(shared)} params</b>shared token encoder · 32→${width}</div><div class=state-node data-tip="${tip('Card-zone Transformer',`${zones*width} zone seeds + ${layers} × ${number(transformer)} Transformer-layer params = ${number(card)}`)}" tabindex=0><b>${number(card)} params</b>card-zone Transformer</div><div class=state-node data-tip="${tip('Entity pooling',`${(collections-1)*width} collection states + ${2*width} shared LayerNorm = ${number(entity)}`)}" tabindex=0><b>${number(entity)} params</b>entity pooling</div><div class=state-node data-tip="${tip('Action input encoder',`${layout.action_values}×${width} value projection + ${width} bias + ${2*width} LayerNorm = ${number(action)}; action tokens reuse the shared token encoder`)}" tabindex=0><b>${number(action)} params</b>action input encoder · ${layout.action_values}+tokens→${width}</div>${actionLayers?`<div class=state-node data-tip="${tip('Action-set Transformer',`${width} learned CLS state + ${actionLayers} × ${number(transformer)} Transformer-layer params = ${number(actionSet)}`)}" tabindex=0><b>${number(actionSet)} params</b>action-set Transformer · ${actionLayers} layer</div>`:''}<div class=state-node data-tip="${tip('Policy head',`(${decisionState}+${width})×${width} hidden layer + biases + ${width}→1 score = ${number(policy)}`)}" tabindex=0><b>${number(policy)} params</b>policy head · state+action→1</div><div class=state-node data-tip="${tip('Win-value head',`${decisionState}×${width} hidden layer + biases + ${width}→1 = ${number(value)}`)}" tabindex=0><b>${number(value)} params</b>win-value head · state→1</div><div class=state-node data-tip="${tip('Progress-value head',`${decisionState}×${width} hidden layer + biases + ${width}→1 = ${number(value)}`)}" tabindex=0><b>${number(value)} params</b>progress-value head · state→1</div><div class=state-node data-tip="${tip('Whole model',`calculated ${number(total)} parameters; manifest records ${number(manifest.parameters)}`)}" tabindex=0><b>${number(total)} params</b>complete model</div></div><div class=state-inspectors><details class=state-box open><summary>Direct global floats — hover any of ${globals}</summary>${globalHtml}</details><details class=state-box open><summary>Raw token fields — hover any of 32</summary><label>Token collection <select id=tokenMode>${[['card','Card'],['enemy','Enemy'],['power','Power'],['delayed','Delayed effect'],['relic','Relic'],['potion','Potion'],['continuation','Continuation'],['map','Map'],['crystal','Crystal'],['encounter','Encounter']].map(([value,label])=>`<option value=${value}>${label}</option>`).join('')}</select></label><div id=tokenGrid class="float-grid token"></div></details></div><details class=state-box open><summary>Learned latent floats — hover any state dimension</summary><p class=state-note>These dimensions are learned mixtures. Their tooltip identifies the exact zone, collection, or action-set coordinate.</p><div class=latent-columns><div><h3>Card-zone state · ${zones*width} floats</h3>${latentRows(cardStart,zoneNames,'#e879f9','Transformer zone-seed output over every encoded card in this zone',width)}</div><div><h3>Non-card entity state · ${(collections-1)*width} floats</h3>${latentRows(entityStart,entityNames,'#34d399','LayerNorm(learned collection state + indexed token sum / sqrt(count))',width)}${actionLayers?`<h3>Action-set context · ${width} floats</h3>${latentRows(baseState,['Action-set summary'],'#f59e0b','Action Transformer CLS output over the full represented action set',width)}`:''}</div></div></details>`;const select=document.querySelector('#tokenMode');select.onchange=()=>renderToken(select.value);renderToken(select.value)}
function flowNode([title,summary,details,color]){return `<div class=flow-node style="--cell:${color}" data-tip="${escapeAttribute(`${title}\n${details}`)}" tabindex=0><b>${title}</b><small>${summary}</small></div>`}function flowRow(nodes){return `<div class=flow-row>${nodes.map(flowNode).join('<div class=flow-arrow>→</div>')}</div>`}
function renderTraining(run){const target=document.querySelector('#trainingDiagram'),manifest=run.manifest,report=run.reports.at(-1),training=manifest.sessions?.at(-1)?.training||manifest.training||{},metrics=report.metrics||{},architecture=manifest.architecture||{},stage=report.stage||{},stages=manifest.stages||[],n=(value,digits=0)=>Number.isFinite(Number(value))?Number(value).toLocaleString(undefined,{maximumFractionDigits:digits}):'—',percent=value=>Number.isFinite(Number(value))?`${(100*Number(value)).toFixed(1)}%`:'—',envs=training.envs||0,samplerSteps=training.sampler_steps??training.rollout_steps??0,slice=envs&&samplerSteps?envs*samplerSteps:0,batch=training.batch||0,replay=architecture.winning_reservoir||0,progress=architecture.value_heads?.includes('progress'),actionLayers=architecture.action_layers||0,current=`A${stage.ascension??'?'} / +${stage.bonus??'?'}`,stageIndex=stage.index??0,next=stages[stageIndex+1],promotion=run.promotions.at(-1),diagrams=[];if(!('sampler_steps' in training)){const steps=report.pipeline||[];target.innerHTML=`<p><strong>Archived training design:</strong> this run predates the continuous asynchronous pipeline.</p><div class=training-diagrams><div class=training-diagram><h3>Reported pipeline</h3>${flowRow(steps.map((step,index)=>[`Step ${index+1}`,step,`${step}\nConfiguration is preserved in this run's manifest.`,['#38bdf8','#818cf8','#a78bfa','#e879f9','#22c55e'][index%5]]))}</div></div>`;return}diagrams.push(`<div class=training-diagram><h3>1. Asynchronous experience collection</h3>${flowRow([['Curriculum',`${current} · stage ${stageIndex+1}/${stages.length||'?'}`,`The collector creates environments at ascension ${stage.ascension??'?'} with training bonus ${stage.bonus??'?'}. Five characters share the same learner. Next stage: ${next?`A${next.ascension}/+${next.bonus}`:'none'}.`,'#38bdf8'],['CPU actor',`${n(envs)} parallel environments`,`One CPU collector thread runs ${n(envs)} simulator environments in fp32. Actions are sampled from the legal-action-masked categorical policy. Training seed: ${training.training_seed??'?'}.`,'#60a5fa'],['Sampler slice',slice?`≤${n(slice)} decisions/call`:`${n(samplerSteps)} steps/environment`,`At most ${n(samplerSteps)} synchronous simulator steps are taken per environment, so a call produces at most ${n(slice)} decisions. Collection stops earlier when a complete episode becomes available. This is not a fixed-length rollout.`,'#818cf8'],['Complete trajectories',`${n(metrics.completed_trajectories)} accepted`,`Per-environment partial trajectories stay with the actor until terminal. Max ${n(training.max_steps)} steps/run and ${n(training.max_combat_steps)} steps/combat; truncated or empty-action trajectories are discarded. Latest discarded fraction: ${percent(metrics.discarded_step_fraction)}.`,'#a78bfa'],['Bounded queues',`weights 1 · samples 2`,`The learner publishes only when the one-slot model queue is empty; the actor drains to the newest version. Complete-trajectory packets cross a two-slot sample queue. Actor and accelerator learner run asynchronously.`,'#c084fc'],['Experience dataset',`peak ${n(metrics.dataset_peak)} rows`,`Latest accepted ${n(metrics.accepted_decisions)} of ${n(metrics.sampled_decisions)} sampled decisions. The transient dataset reached ${n(metrics.dataset_peak)} rows and is drained exactly once.`,'#e879f9']])}</div>`);diagrams.push(`<div class=training-diagram><h3>2. Returns, priorities, and minibatch selection</h3>${flowRow([['Three reward streams','win · shaped · progress',`Win reward is the terminal simulator reward. Policy reward adds the potential difference Vpotential(next) − Vpotential(current). Progress reward is the increase in maximum floor reached, divided by 52.`,'#22c55e'],['GAE and targets',`λ=${training.gae_lambda??'?'} · γprogress=${training.progress_gamma??'?'}`,`Policy advantage uses shaped reward with γ=1. Win-value return uses raw terminal reward with γ=1 and is clipped to [0,1]. Progress advantage and target use γ=${training.progress_gamma??'?'}; all use GAE λ=${training.gae_lambda??'?'}.`,'#14b8a6'],['Priority',`1+|Awin|+|Aprogress|+terminal/win bonuses`,`Each row gets priority 1 + |policy advantage| + |progress advantage| + 4×terminal + 4×winning-return. Sampling uses the square root of priority.`,'#06b6d4'],['Token-cost bucket',`one bucket per batch`,`Rows are bucketed by floor(log2(estimated token/attention cost)). A bucket is chosen by total priority to reduce padding and attention waste.`,'#0ea5e9'],['Balanced one-pass batch',`up to ${n(batch)} rows`,`Each batch draws evenly across the five characters when possible, without replacement, then deletes those rows from the dataset. Latest attempted/trained: ${n(metrics.dataset_attempted)} / ${n(metrics.dataset_trained)}.`,'#38bdf8'],['Freshness filters',`lag ≤${n(training.max_policy_lag)} · |log ratio| ≤${training.max_log_ratio??'?'}`,`Rows older than ${n(training.max_policy_lag)} learner updates are pruned. After forward evaluation, rows with |new logπ(a) − old logπ(a)| > ${training.max_log_ratio??'?'} are masked. Latest stale/ratio drops: ${n(metrics.dataset_stale_dropped)} / ${n(metrics.dataset_ratio_dropped)}.`,'#818cf8']])}</div>`);diagrams.push(`<div class=training-diagram><h3>3. PPO learner update</h3>${flowRow([['Model forward',`${architecture.layers??'?'} card layer${actionLayers?` + ${actionLayers} action-set layer`:''}`,`The selected model encodes the state and every represented action, then emits one masked policy logit per action, a win logit, and ${progress?'a progress value':'no progress head'}. Precision: ${training.precision??manifest.precision??'?'}.`,'#a78bfa'],['Normalize advantages',`per character · β=${n(metrics.progress_beta,3)}`,`Fresh win and progress advantages are independently z-normalized inside each character. Policy advantage = z(Awin) + β·z(Aprogress). β decays from 1 to 0.1 over ${n(training.progress_decisions)} decisions while the progress curriculum remains active.`,'#c084fc'],['Combined loss',`PPO ε=${training.clip??'?'} · value weight ${training.value_weight??'?'}`,`Loss = clipped PPO policy loss + ${training.value_weight??'?'}×win BCE${progress?` + ${training.value_weight??'?'}×progress MSE`:''} − entropy_weight×entropy. Entropy decays ${training.entropy_start??'?'} → ${training.entropy_end??'?'} over ${n(training.progress_decisions)} decisions.`,'#e879f9'],['Batch KL gate',`KL ≤ ${training.target_kl??'?'}`,`Mean sampled-action KL estimate is ratio − 1 − log_ratio over fresh rows. If it exceeds ${training.target_kl??'?'}, the entire optimizer step is skipped. Latest KL: ${n(metrics.kl,6)}; KL-dropped rows: ${n(metrics.dataset_kl_dropped)}.`,'#f472b6'],['Backpropagation',`global gradient norm ≤ 0.5`,`Gradients from fresh PPO rows and accepted winning replay are accumulated together, then clipped to global norm 0.5.`,'#f97316'],['Adam + publish',`lr ${training.learning_rate??'?'}`,`Adam uses learning rate ${training.learning_rate??'?'} and eps 1e-5. After a successful step, policy version increments and weights are offered to the actor. Latest learner/sampler versions: ${n(metrics.policy_version)} / ${n(metrics.sampler_version)}; mean lag ${n(metrics.policy_lag_mean,2)}.`,'#f59e0b']])}</div>`);if(replay)diagrams.push(`<div class=training-diagram><h3>4. Terminal winning replay</h3>${flowRow([['Pending episode sample',`per-env reservoir sampling`,`During each live episode, candidate decisions are reservoir-sampled into a bounded per-environment pending set. Stored data includes the chosen action, behavior log-probability, win value, and full behavior policy.`,'#84cc16'],['Admit winning rows',`${n(metrics.winning_reservoir)}/${n(replay)} stored`,`Only completed wins are admitted, and rows with advantage 1−Vwin ≤0 are skipped. Global capacity is ${n(replay)} with reservoir replacement. Latest wins/seen/skipped: ${n(metrics.winning_episodes)} / ${n(metrics.winning_seen)} / ${n(metrics.winning_skipped)}.`,'#22c55e'],['Balanced compatible sample','≤10% of update data',`Candidates are round-robin balanced across characters. At most fresh_batch/9 rows survive, making replay at most 10% of fresh+replay data. Only candidates no more expensive than the fresh token-cost bucket are packed.`,'#14b8a6'],['Categorical KL check',`≤${training.target_kl??'?'}`,`The stored full behavior distribution is compared with the current masked policy. Eligible rows above KL ${training.target_kl??'?'} are rejected and their reference policy is re-anchored to the current model. Latest accepted/rejected: ${n(metrics.winning_replayed)} / ${n(metrics.winning_rejected_kl)}.`,'#06b6d4'],['Replay PPO term',`weight ${metrics.winning_loss_weight??.1}`,`Replay advantage is stop_gradient(1 − sigmoid(win logit)). The selected-action ratio uses the stored reference policy and the same PPO clip ε=${training.clip??'?'}. Its loss is multiplied by ${metrics.winning_loss_weight??.1}.`,'#0ea5e9']])}</div>`);diagrams.push(`<div class=training-diagram><h3>${replay?'5':'4'}. Reporting, checkpoints, and promotion</h3>${flowRow([['Live report',`every ${n(training.report_decisions)} decisions`,`At each handled-decision boundary, metrics are aggregated, live.json and an immutable report are written, latest.pt is refreshed, and this dashboard is regenerated. Latest step: ${n(report.step)}.`,'#38bdf8'],['Best checkpoint',`score improves lexicographically`,`A report becomes best when it improves: characters with any win; then worst per-character floor penalized by caps; then mean floor; then aggregate win rate.`,'#60a5fa'],['Periodic checkpoint',`every ${n(training.save_decisions)} decisions`,`An immutable numbered checkpoint plus latest.pt and SHA-256 metadata are written every ${n(training.save_decisions)} handled decisions.`,'#818cf8'],['Promotion trigger',`${n(training.promotion_window)} episodes/character`,`The last ${n(training.promotion_window)} training episodes for every character must each reach at least ${percent(training.promote_win_rate)} wins. This stops the actor and starts deterministic evaluation.`,'#a78bfa'],['Greedy evaluation',`${n(training.promotion_runs)} runs × 5 characters`,`Argmax actions run in batches of ${n(training.evaluation_batch)}, with ${n(training.evaluation_max_steps)}-step run and ${n(training.evaluation_max_combat_steps)}-step combat caps. Promotion seed advances independently.`,'#c084fc'],['Stage outcome',promotion?`${promotion.promoted?'promoted':'stayed'} at ${n(promotion.step)}`:'no check yet',`Every character must win at least ${percent(training.promote_win_rate)} of all evaluation runs. Pass: advance to ${next?`A${next.ascension}/+${next.bonus}`:'the final stage'} and clear winning replay. Once every character reaches 10%, the auxiliary progress advantage is permanently disabled. Training stops at ${training.hours?`${training.hours} hour${training.hours===1?'':'s'}`:'no time cap'} or ${training.decisions?n(training.decisions)+' decisions':'no decision cap'}, whichever comes first.`,'#e879f9']])}</div>`);target.innerHTML=`<p><strong>Latest report:</strong> ${n(report.step)} decisions · ${n(metrics.updates)} optimizer updates · ${n(metrics.decisions_per_second,1)} decisions/s</p><div class=training-diagrams>${diagrams.join('')}</div>`}
const stateTip=document.querySelector('#stateTip');function showStateTip(cell,x,y){stateTip.textContent=cell.dataset.tip;stateTip.style.display='block';stateTip.style.left=Math.min(x+14,innerWidth-stateTip.offsetWidth-12)+'px';stateTip.style.top=Math.min(y+14,innerHeight-stateTip.offsetHeight-12)+'px'}document.addEventListener('pointerover',event=>{const cell=event.target.closest('[data-tip]');if(cell)showStateTip(cell,event.clientX,event.clientY)});document.addEventListener('pointermove',event=>{const cell=event.target.closest('[data-tip]');if(cell)showStateTip(cell,event.clientX,event.clientY)});document.addEventListener('pointerout',event=>{if(event.target.closest('[data-tip]'))stateTip.style.display='none'});document.addEventListener('focusin',event=>{const cell=event.target.closest('[data-tip]');if(cell){const rect=cell.getBoundingClientRect();showStateTip(cell,rect.right,rect.bottom)}});document.addEventListener('focusout',event=>{if(event.target.closest('[data-tip]'))stateTip.style.display='none'});
function details(run){const manifest=run.manifest,report=run.reports.at(-1),training=manifest.sessions?.at(-1)?.training||manifest.training||{},heads=manifest.architecture?.value_heads||[],progress=heads.includes('progress'),replay=manifest.architecture?.winning_reservoir,weight=training.value_weight??'?',replayWeight=report.metrics.winning_loss_weight??.1,architecture=Object.entries(manifest.architecture||{}).map(([key,value])=>`${key}=${Array.isArray(value)?value.join('/'):value}`).join(' · '),metrics=Object.entries(report.metrics).filter(([,value])=>typeof value==='number').map(([key,value])=>`<tr><td>${key}</td><td>${Number(value).toLocaleString(undefined,{maximumSignificantDigits:6})}</td></tr>`).join(''),configuration=['precision','learning_rate','batch','envs','sampler_steps','rollout_steps','clip','gae_lambda','progress_gamma','progress_decisions','target_kl'].filter(key=>key in training).map(key=>`${key}=${training[key]}`).join(' · '),entropy=`${training.entropy_start??'?'}→${training.entropy_end??'?'}${training.progress_decisions?` over ${Number(training.progress_decisions).toLocaleString()} decisions`:''}`,advantage=progress?`per-character z(Awin) + β·z(Aprogress); GAE γwin=1, γprogress=${training.progress_gamma}, λ=${training.gae_lambda}; β=1→0.1 while active`:`win GAE γ=1, λ=${training.gae_lambda}`,winLoss=heads.includes('win_logit')?'terminal win/death BCE':'terminal win/death value regression',loss=`PPO-clip ε=${training.clip} + ${weight}×${winLoss}${progress?` + ${weight}×progress MSE`:''} − entropy×H${replay?` + ${replayWeight}×winning-replay PPO`:''}`,replayRule=replay?`; replay advantage=1−Vwin, data≤10%, categorical KL≤${training.target_kl}`:'';return `<p><strong>${manifest.change||report.description}</strong></p><p>${report.description}</p><p><strong>Architecture</strong> ${architecture}</p><p><strong>Training</strong> ${configuration}</p><p><strong>Entropy schedule</strong> ${entropy}</p><p><strong>Advantage</strong> ${advantage}</p><p><strong>Loss</strong> ${loss}${replayRule}</p><div class=pipeline>${report.pipeline.map(step=>`<span>${step}</span>`).join('')}</div><details><summary>Latest measured metrics</summary><table><tbody>${metrics}</tbody></table></details>`}
function promotionSummary(row){if(!row)return '<p>No promotion check yet.</p>';const characters=row.result?.characters||[],rows=characters.map(item=>`<tr><td>${characterNames[item.character]??`Character ${item.character}`}</td><td>${item.wins}/${item.runs}</td><td>${(100*item.wins/item.runs).toFixed(1)}%</td><td>${Number(item.floor_mean).toFixed(2)}</td><td>${item.caps}</td></tr>`).join('');return `<p class="${row.promoted?'yes':'no'}">${row.promoted?'Promoted':'Stayed at current stage'} · threshold ${(100*row.threshold).toFixed(0)}% per character · seed ${row.seed}</p><table><thead><tr><th>Character</th><th>Wins</th><th>Rate</th><th>Mean floor</th><th>Caps</th></tr></thead><tbody>${rows}</tbody></table>`}
function showVersion(){const run=versions[versionSelect.value],reports=run.reports;document.querySelector('#title').textContent=`Spirefysh V${run.version}`;document.querySelector('#details').innerHTML=details(run);renderState(run);renderTraining(run);plot('advantage',series(reports,'mean_advantage'),{history:reports,run});plot('throughput',series(reports,'decisions_per_second'),{tozero:true,history:reports,run});floorPlot(reports,run);stagePlot('ascension',reports,'ascension','#fb7185',run);stagePlot('bonus',reports,'bonus','#f59e0b',run);plot('wins',reports.map(row=>({x:x(row),y:row.metrics.wins/Math.max(1,row.metrics.episodes)})),{range:[0,1],percent:true,history:reports,run});plot('clip',series(reports,'clip_fraction'),{range:[0,1],percent:true,history:reports,run});plot('kl',series(reports,'kl'),{tozero:true,history:reports,run});plot('entropy',series(reports,'entropy'),{tozero:true,history:reports,run});document.querySelector('#promotion').innerHTML=promotionSummary(run.promotions.at(-1))}versionSelect.onchange=showVersion;xaxis.onchange=showVersion;smooth.onchange=showVersion;ema.oninput=showVersion;showVersion()</script>"""
    target.mkdir(parents=True, exist_ok=True)
    temporary = target / "dashboard.html.tmp"
    temporary.write_text(content)
    temporary.replace(target / "dashboard.html")


def train(args):
    torch.manual_seed(args.seed); np.random.seed(args.seed)
    target = device()
    if args.precision == "bf16" and target.type not in ("mps", "cuda"):
        raise ValueError("bf16 requires MPS or CUDA")
    if min(args.envs, args.batch, args.sampler_steps, args.report_decisions,
           args.save_decisions, args.promotion_window, args.promotion_runs,
           args.max_policy_lag + 1) < 1 or args.max_log_ratio <= 0:
        raise ValueError("invalid asynchronous replay settings")
    if args.hours <= 0 and args.decisions <= 0:
        raise ValueError("set --hours or --decisions")
    probe_env = sts2_sim.Batch(1, args.training_seed, None, ascension=STAGES[0][0])
    layout = dict(probe_env.token_layout())
    source = None
    if args.checkpoint:
        model, source = load(args.checkpoint, target)
        requested = (args.width, args.layers, args.heads, args.feedforward)
        loaded = (model.width, model.layers, model.heads, model.feedforward)
        if any(value is not None and value != saved for value, saved in zip(requested, loaded)):
            raise ValueError(f"checkpoint architecture {loaded} does not match requested {requested}")
        args.width, args.layers, args.heads, args.feedforward = loaded
    else:
        config = tuple(value if value is not None else default for value, default in zip(
            (args.width, args.layers, args.heads, args.feedforward), (128, 2, 4, 256)
        ))
        args.width, args.layers, args.heads, args.feedforward = config
        model = Agent(layout, *config).to(target)
    fused_optimizer = target.type != "cpu"
    optimizer = torch.optim.Adam(
        model.parameters(), lr=args.learning_rate, eps=1e-5, fused=fused_optimizer
    )
    optimizer_restored = bool(source and source["_optimizer_compatible"])
    if optimizer_restored:
        optimizer.load_state_dict(source["optimizer"])
        for group in optimizer.param_groups:
            group["lr"] = args.learning_rate
            group["fused"] = fused_optimizer
            group["foreach"] = None
        torch.set_rng_state(source["torch_rng"].cpu())
    output = Path(args.output)
    continuing = output.exists()
    if continuing and (not source or Path(args.checkpoint).resolve() != (output / "latest.pt").resolve()):
        raise FileExistsError(f"{output} exists; continue it from its latest.pt")
    training = {key: value for key, value in vars(args).items() if key != "command"}
    manifest = json.loads((output / "run.json").read_text()) if continuing else {
        "schema": 1, "model_version": MODEL_VERSION, "feature_version": FEATURE_VERSION,
        "fingerprint": probe_env.fingerprint(), "layout": layout, "precision": args.precision,
        "change": CHANGE, "parameters": sum(parameter.numel() for parameter in model.parameters()),
        "architecture": architecture(model),
        "stages": [{"ascension": ascension, "bonus": bonus} for ascension, bonus in STAGES],
        "training": training,
        "source": args.checkpoint, "optimizer_restored": optimizer_restored,
        "resume": "warm continuation; learner, optimizer and reservoir restored; sampler restarted" if source else None,
    }
    manifest["change"] = CHANGE
    manifest["precision"] = args.precision
    output.mkdir(parents=True, exist_ok=continuing)
    manifest.setdefault("sessions", []).append({"step": source["decisions"] if source else 0, "training": training})
    atomic_json(output / "run.json", manifest)
    reports_dir = output / "reports"; reports_dir.mkdir(exist_ok=continuing)
    checkpoints_dir = output / "checkpoints"; checkpoints_dir.mkdir(exist_ok=continuing)
    best_dir = output / "best"; best_dir.mkdir(exist_ok=continuing)
    promotions_dir = output / "promotions"; promotions_dir.mkdir(exist_ok=continuing)
    stage = source["stage"] if source else 0
    reservoir = WinningReservoir(WINNING_CAPACITY, args.envs)
    if source and source.get("winning_reservoir"):
        reservoir.load_state_dict(source["winning_reservoir"])
        missing = [index for index, row in enumerate(reservoir.rows) if row[4] is None]
        for start in range(0, len(missing), 128):
            indices = missing[start:start + 128]
            with torch.no_grad():
                inputs = unpack([reservoir.rows[index][0] for index in indices], target, model)
                logits = predict(model, inputs, args.precision)[0].masked_fill(~inputs[6], -torch.inf)
                policies = logits.log_softmax(-1).cpu().numpy()
            for index, policy in zip(indices, policies):
                row = reservoir.rows[index]
                reservoir.rows[index] = (*row[:4], np.asarray(policy[:len(row[0][3])], np.float16))
    decisions = source["decisions"] if source else 0
    auxiliary_decisions = source.get("auxiliary_decisions", 0) if source else 0
    progress_active = source.get("progress_active", True) if source else True
    sampler_session = source.get("sampler_session", source.get("sampler_index", 0)) if source else 0
    promotion_index = source.get("promotion_index", 0) if source else 0
    if not 0 <= stage < len(STAGES):
        raise ValueError("invalid checkpoint stage")
    if args.decisions and args.decisions < decisions:
        raise ValueError("--decisions precedes the checkpoint")
    if args.decisions and (args.decisions - decisions) % args.envs:
        raise ValueError("remaining decisions must be divisible by environments")
    latest = output / "latest.pt"
    def save(path, step, replace):
        return save_checkpoint(
            path, model, optimizer, manifest, stage, step, sampler_session,
            promotion_index, progress_active, reservoir, auxiliary_decisions + step - decisions,
            replace,
        )
    digest = save(latest, decisions, True)
    atomic_json(output / "latest.json", {
        "step": decisions, "stage": stage, "sampler_session": sampler_session,
        "checkpoint": latest.name, "sha256": digest,
    })
    dashboard(output.parent)
    started = time.monotonic()
    elapsed_offset = max((json.loads(path.read_text())["metrics"].get("seconds", 0)
                          for path in reports_dir.glob("*.json")), default=0) if continuing else 0
    run_started = started - elapsed_offset
    deadline = started + args.hours * 3600 if args.hours else math.inf
    training_seconds = promotion_seconds = 0.0
    best_score = tuple(json.loads((output / "best.json").read_text())["score"]) if (output / "best.json").exists() else ()
    while time.monotonic() < deadline and (not args.decisions or decisions < args.decisions):
        sampler_session += 1
        base = decisions
        budget = args.decisions - decisions if args.decisions else (1 << 62) // args.envs * args.envs
        def save_latest(step):
            digest = save(latest, step, True)
            atomic_json(output / "latest.json", {
                "step": step, "stage": stage, "sampler_session": sampler_session,
                "checkpoint": latest.name, "sha256": digest,
            })
        def save_step(step):
            checkpoint = checkpoints_dir / f"{step:012}.pt"
            immutable_digest = save(checkpoint, step, False)
            digest = save(latest, step, True)
            atomic_json(output / "latest.json", {
                "step": step, "stage": stage, "sampler_session": sampler_session,
                "checkpoint": latest.name,
                "immutable": str(checkpoint.relative_to(output)),
                "immutable_sha256": immutable_digest, "sha256": digest,
            })
        def save_report(point, pipeline, window):
            nonlocal best_score
            row = {
                "schema": 1, "step": point["steps"], "window": window,
                "sampler_session": sampler_session,
                "stage": {"index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1]},
                "description": f"Continuous V{MODEL_VERSION} training at A{STAGES[stage][0]}/+{STAGES[stage][1]}.",
                "pipeline": pipeline, "metrics": point,
            }
            immutable_json(reports_dir / f"{point['steps']:012}.json", row)
            atomic_json(output / "live.json", row)
            save_latest(point["steps"])
            characters = point["characters"]
            score = (
                sum(character["wins"] > 0 for character in characters),
                min(character["floor_mean"] - 17 * character["caps"] / max(1, character["episodes"])
                    for character in characters),
                point["floor_bands"]["mean"], point["wins"] / max(1, point["episodes"]),
            )
            if score > best_score:
                checkpoint = best_dir / f"{point['steps']:012}.pt"
                digest = save(checkpoint, point["steps"], False)
                best_score = score
                atomic_json(output / "best.json", {
                    "step": point["steps"], "score": score,
                    "checkpoint": str(checkpoint.relative_to(output)), "sha256": digest,
                })
            dashboard(output.parent)
            print(json.dumps(row), flush=True)
        model.train()
        training = train_stream(
            model, optimizer, args, sampler_session, stage, target, deadline, budget,
            base, auxiliary_decisions, run_started, reservoir, progress_active,
            save_report, save_step,
        )
        if not training["decisions"]:
            break
        decisions += training["decisions"]
        auxiliary_decisions += training["decisions"]
        training_seconds += training["seconds"]
        if not training["promotion_ready"] or time.monotonic() >= deadline:
            break
        promotion_started = time.monotonic()
        seed = args.promotion_seed + promotion_index * 100_000_000 + stage * 10_000_000
        result = evaluate(
            model, args, target, seed, args.promotion_runs, stage,
            args.evaluation_max_steps, args.evaluation_max_combat_steps,
        )
        promoted = all(row["wins"] / row["runs"] >= args.promote_win_rate for row in result["characters"])
        meaningful = all(row["wins"] / row["runs"] >= .1 for row in result["characters"])
        promotion_index += 1
        promotion = {
            "schema": 1, "step": decisions, "seed": seed,
            "stage": {"index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1]},
            "threshold": args.promote_win_rate, "promoted": promoted, "result": result,
        }
        immutable_json(promotions_dir / f"{decisions:012}.json", promotion)
        progress_active &= not meaningful
        if promoted:
            stage += 1
            reservoir.clear()
        promotion_seconds += time.monotonic() - promotion_started
        save_latest(decisions)
        dashboard(output.parent)
        print(json.dumps(promotion), flush=True)
    digest = save(latest, decisions, True)
    atomic_json(output / "latest.json", {
        "step": decisions, "stage": stage, "sampler_session": sampler_session,
        "checkpoint": latest.name, "sha256": digest,
    })
    atomic_json(output / "timing.json", {
        "elapsed_seconds": elapsed_offset + time.monotonic() - started,
        "training_seconds": training_seconds, "promotion_seconds": promotion_seconds,
        "training_fraction": training_seconds / max(1e-9, training_seconds + promotion_seconds),
        "decisions": decisions, "stage": stage,
    })
    dashboard(output.parent)


def load(path, target):
    checkpoint = torch.load(path, map_location=target, weights_only=False)
    live = sts2_sim.Batch(1, 0, None, ascension=0)
    layout = dict(live.token_layout())
    if checkpoint.get("schema") != 1 or checkpoint.get("model_version") != MODEL_VERSION:
        raise ValueError("incompatible checkpoint")
    if checkpoint.get("feature_version") != FEATURE_VERSION or layout["version"] != FEATURE_VERSION:
        raise ValueError("incompatible feature version")
    if checkpoint["fingerprint"] != live.fingerprint() or checkpoint["layout"] != layout:
        raise ValueError("checkpoint does not match simulator content")
    config = checkpoint.get("architecture", {})
    try:
        model = Agent(layout, *(config[key] for key in (
            "width", "layers", "heads", "feedforward", "action_layers"
        ))).to(target)
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError("invalid checkpoint architecture") from error
    if config != architecture(model):
        raise ValueError("incompatible model architecture")
    model.load_state_dict(checkpoint["model"])
    checkpoint["_optimizer_compatible"] = migrate_optimizer(checkpoint["optimizer"], len(tuple(model.parameters())))
    return model, checkpoint


def value_dataset(model, args, target, seed, runs, deadline):
    states = []; labels = []; characters = []
    reports = []
    model.eval()
    for character in range(5):
        outcomes = []; floors = []; caps = 0
        for start in range(0, runs, args.batch):
            if time.monotonic() >= deadline:
                raise TimeoutError("finalization exceeded --hours")
            count = min(args.batch, runs - start)
            env = sts2_sim.Batch(count, seed + character * 1_000_000 + start, character, ascension=10)
            active = np.ones(count, bool)
            episode_steps = np.zeros(count, np.int32)
            combat_steps = np.zeros(count, np.int32)
            pending = [[] for _ in range(count)]
            seen = np.zeros(count, np.int32)
            rng = np.random.default_rng(seed + character * 1_000_000 + start)
            for _ in range(args.max_steps):
                if time.monotonic() >= deadline:
                    raise TimeoutError("finalization exceeded --hours")
                if not active.any():
                    break
                observation = env.observe_tokens(active.tolist())
                legal = np.asarray(observation[6], bool)
                empty = active & ~legal.any(1)
                for index in np.flatnonzero(empty):
                    pending[index].clear()
                caps += int(empty.sum()); active &= ~empty
                if not active.any():
                    break
                inputs = tensors(observation, target, model)
                with torch.no_grad(), torch.autocast(
                    target.type, dtype=torch.bfloat16,
                    enabled=args.precision == "bf16",
                ):
                    logits, _, _, encoded = model(*inputs[:6], inputs[7], return_state=True)
                masked = logits.float().masked_fill(~inputs[6], -torch.inf)
                choice = masked.argmax(1).cpu().numpy()
                encoded = encoded.float().cpu().numpy().astype(np.float16)
                for index in np.flatnonzero(active):
                    seen[index] += 1
                    if len(pending[index]) < args.states_per_run:
                        pending[index].append(encoded[index])
                    else:
                        slot = int(rng.integers(seen[index]))
                        if slot < args.states_per_run:
                            pending[index][slot] = encoded[index]
                in_combat = np.asarray([row[4] == 1 for row in env.stats()])
                combat_steps = np.where(active & in_combat, combat_steps + 1, 0)
                _, done, _ = env.step(choice.tolist(), active.tolist())
                done = np.asarray(done, bool); episode_steps += active
                stats = env.stats()
                still_combat = np.asarray([row[4] == 1 for row in stats])
                capped = active & ~done & (
                    (episode_steps >= args.max_steps)
                    | ((combat_steps >= args.max_combat_steps) & still_combat)
                )
                caps += int(capped.sum())
                for index in np.flatnonzero(done):
                    won = int(stats[index][4] == 12)
                    states.extend(pending[index]); labels.extend([won] * len(pending[index]))
                    characters.extend([character] * len(pending[index]))
                    outcomes.append(won); floors.append((stats[index][0] - 1) * 17 + stats[index][1])
                for index in np.flatnonzero(capped):
                    pending[index].clear()
                active &= ~(done | capped)
        reports.append({
            "character": character, "runs": runs, "resolved": len(outcomes),
            "wins": sum(outcomes), "win_rate": sum(outcomes) / max(1, runs),
            "caps": caps, "floor_mean": float(np.mean(floors)) if floors else 0,
        })
    if not states:
        raise RuntimeError("no resolved trajectories")
    return np.asarray(states, np.float16), np.asarray(labels, np.float32), np.asarray(characters), reports


def value_logits(model, states, batch, target):
    values = []
    model.eval()
    with torch.no_grad():
        for start in range(0, len(states), batch):
            inputs = torch.as_tensor(states[start : start + batch], dtype=torch.float32, device=target)
            values.extend(model.value(inputs).squeeze(-1).cpu().tolist())
    return np.asarray(values, np.float32)


def calibration(logits, labels):
    logits = torch.as_tensor(logits)
    labels = torch.as_tensor(labels)
    log_temperature = torch.zeros((), requires_grad=True)
    bias = torch.zeros((), requires_grad=True)
    optimizer = torch.optim.Adam([log_temperature, bias], lr=.05)
    for _ in range(500):
        optimizer.zero_grad()
        loss = nn.functional.binary_cross_entropy_with_logits(
            logits / log_temperature.exp() + bias, labels
        )
        loss.backward()
        optimizer.step()
        with torch.no_grad():
            log_temperature.clamp_(-5, 5); bias.clamp_(-15, 15)
    return float(log_temperature.detach().exp()), float(bias.detach())


def value_metrics(logits, labels, characters, temperature, bias, base_rates):
    probabilities = 1 / (1 + np.exp(-np.clip(logits / temperature + bias, -80, 80)))
    baseline = np.asarray([base_rates[character] for character in characters])

    def scores(selected):
        if not selected.any():
            return {"states": 0, "base_rate": None, "brier": None, "ece10": None,
                    "constant_brier": None, "beats_constant": False}
        expected = labels[selected]; predicted = probabilities[selected]
        ece = 0.0
        for index in range(10):
            member = (predicted >= index / 10) & (predicted < (index + 1) / 10)
            if index == 9:
                member |= predicted == 1
            if member.any():
                ece += member.mean() * abs(predicted[member].mean() - expected[member].mean())
        brier = float(np.mean((predicted - expected) ** 2))
        constant = float(np.mean((baseline[selected] - expected) ** 2))
        return {
            "states": int(selected.sum()), "base_rate": float(expected.mean()),
            "brier": brier, "ece10": float(ece), "constant_brier": constant,
            "beats_constant": brier < constant,
        }

    result = scores(np.ones(len(labels), bool))
    result["characters"] = [scores(characters == character) for character in range(5)]
    return result


def export_value_model(path, model, fingerprint, temperature, bias):
    path = Path(path); path.parent.mkdir(parents=True, exist_ok=True)
    parts = [
        b"STSVALUE", struct.pack("<IIQ", MODEL_VERSION, FEATURE_VERSION, fingerprint),
        struct.pack("<10I", model.width, model.layers, model.action_layers,
                    model.heads, model.feedforward,
                    model.layout["globals"], model.layout["collections"],
                    model.layout["categorical"], model.layout["numeric"],
                    model.layout["action_values"]),
        struct.pack("<2f", temperature, bias),
    ]

    def add(value):
        parts.append(value.detach().float().cpu().contiguous().numpy().astype("<f4", copy=False).tobytes())

    for value in (model.embedding.weight, model.numeric.weight,
                  model.token_norm.weight, model.token_norm.bias):
        add(value)
    for transformer in (model.state_transformer, model.decision_transformer):
        for layer in transformer.layers:
            for value in (
                layer.self_attn.in_proj_weight, layer.self_attn.in_proj_bias,
                layer.self_attn.out_proj.weight, layer.self_attn.out_proj.bias,
                layer.norm1.weight, layer.norm1.bias, layer.norm2.weight, layer.norm2.bias,
                layer.linear1.weight, layer.linear1.bias, layer.linear2.weight, layer.linear2.bias,
            ):
                add(value)
    for value in (
        model.action_value.weight, model.action_value.bias,
        model.action_norm.weight, model.action_norm.bias,
        model.state_state, model.decision_state,
        model.value[0].weight, model.value[0].bias, model.value[2].weight, model.value[2].bias,
    ):
        add(value)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_bytes(b"".join(parts)); temporary.replace(path)
    return hashlib.sha256(path.read_bytes()).hexdigest()


def finalize(args):
    torch.manual_seed(args.seed); np.random.seed(args.seed)
    if min(args.fit_runs, args.calibration_runs, args.test_runs, args.states_per_run,
           args.batch, args.head_batch, args.epochs, args.max_steps,
           args.max_combat_steps, args.hours) <= 0:
        raise ValueError("invalid finalization settings")
    if args.test_runs < 1000:
        raise ValueError("final test requires at least 1000 runs per character")
    namespaces = sorted((seed, seed + 4_000_000 + runs) for seed, runs in (
        (args.fit_seed, args.fit_runs), (args.calibration_seed, args.calibration_runs),
        (args.test_seed, args.test_runs),
    ))
    if any(left[1] >= right[0] for left, right in zip(namespaces, namespaces[1:])):
        raise ValueError("fit, calibration and test seeds must be disjoint")
    target = device()
    deadline = time.monotonic() + args.hours * 3600
    model, checkpoint = load(args.checkpoint, target)
    if checkpoint["stage"] != len(STAGES) - 1:
        raise ValueError("finalization requires an A10/+0 checkpoint")
    args.precision = args.precision or checkpoint.get("precision", "fp32")
    if args.precision == "bf16" and target.type not in ("mps", "cuda"):
        raise ValueError("bf16 requires MPS or CUDA")
    fit = value_dataset(model, args, target, args.fit_seed, args.fit_runs, deadline)
    fit_states, fit_report = len(fit[0]), fit[3]
    for parameter in model.parameters():
        parameter.requires_grad_(False)
    for parameter in model.value.parameters():
        parameter.requires_grad_(True)
    optimizer = torch.optim.Adam(model.value.parameters(), lr=args.learning_rate)
    rng = np.random.default_rng(args.seed)
    model.train()
    for _ in range(args.epochs):
        if time.monotonic() >= deadline:
            raise TimeoutError("finalization exceeded --hours")
        order = rng.permutation(len(fit[0]))
        for start in range(0, len(order), args.head_batch):
            index = order[start : start + args.head_batch]
            states = torch.as_tensor(fit[0][index], dtype=torch.float32, device=target)
            labels = torch.as_tensor(fit[1][index], device=target)
            loss = nn.functional.binary_cross_entropy_with_logits(
                model.value(states).squeeze(-1), labels
            )
            optimizer.zero_grad(set_to_none=True); loss.backward(); optimizer.step()
    del fit
    calibrate = value_dataset(model, args, target, args.calibration_seed, args.calibration_runs, deadline)
    calibration_logits = value_logits(model, calibrate[0], args.head_batch, target)
    temperature, bias = calibration(calibration_logits, calibrate[1])
    base_rates = [
        float(calibrate[1][calibrate[2] == character].mean())
        if (calibrate[2] == character).any() else float(calibrate[1].mean())
        for character in range(5)
    ]
    calibration_metrics = value_metrics(
        calibration_logits, calibrate[1], calibrate[2], temperature, bias, base_rates
    )
    calibration_report = calibrate[3]
    del calibrate
    test = value_dataset(model, args, target, args.test_seed, args.test_runs, deadline)
    test_logits = value_logits(model, test[0], args.head_batch, target)
    test_metrics = value_metrics(test_logits, test[1], test[2], temperature, bias, base_rates)
    test_metrics["held_out_win_each_character"] = all(row["wins"] for row in test[3])
    if not test_metrics["held_out_win_each_character"]:
        raise RuntimeError("final test requires a held-out win for every character")
    if not test_metrics["beats_constant"] or not all(
        row["beats_constant"] for row in test_metrics["characters"]
    ):
        raise RuntimeError("final test Brier score must beat the per-character constant baseline")
    output = Path(args.output)
    digest = export_value_model(output, model, checkpoint["fingerprint"], temperature, bias)
    parity_env = sts2_sim.Batch(8, args.test_seed + 99_000_000, None, ascension=10)
    parity_inputs = tensors(parity_env.observe_tokens(), target, model)
    with torch.no_grad():
        python_values = (predict(model, parity_inputs, "fp32")[1] / temperature + bias).sigmoid().cpu().numpy()
    rust_values = np.asarray(parity_env.rust_values(str(output)))
    parity_error = float(np.max(np.abs(python_values - rust_values)))
    if parity_error > 1e-5:
        raise RuntimeError(f"Rust inference differs by {parity_error}")
    final_checkpoint = checkpoint | {
        "model": model.state_dict(), "optimizer": torch.optim.Adam(model.parameters()).state_dict(),
        "finalized": True,
        "calibration": {"temperature": temperature, "bias": bias},
    }
    final_checkpoint.pop("_optimizer_compatible", None)
    torch.save(final_checkpoint, output.with_suffix(".pt"))
    report = {
        "schema": 1, "model_version": MODEL_VERSION, "feature_version": FEATURE_VERSION,
        "source": args.checkpoint, "output": str(output), "sha256": digest,
        "policy": "frozen greedy A10/+0", "temperature": temperature, "bias": bias,
        "rust_pytorch_max_abs": parity_error,
        "fit": {"trajectories": fit_report, "states": fit_states},
        "calibration": calibration_metrics | {"trajectories": calibration_report, "base_rates": base_rates},
        "test": test_metrics | {"trajectories": test[3]},
        "supported_states": "states visited by this frozen player-visible policy",
    }
    atomic_json(output.with_suffix(".json"), report)
    print(json.dumps(report, indent=2))


def probe():
    torch.manual_seed(1); np.random.seed(1)
    target = device()
    env = sts2_sim.Batch(8, 1, None, ascension=0)
    layout = dict(env.token_layout())
    observation = env.observe_tokens()
    model = Agent(layout).to(target)
    reservoir = WinningReservoir(2, 1)
    row = pack(observation, 0)
    replay_rng = np.random.default_rng(1)
    policy = np.full(len(row[3]), -np.log(len(row[3])), np.float32)
    reservoir.record([row], [0], [0], [policy], [0], replay_rng)
    reservoir.record([row], [0], [0], [policy], [2], replay_rng)
    assert reservoir.finish([0], [True], replay_rng) == 1
    replay = reservoir.sample(9 // 9, np.random.default_rng(2), model.character_start)
    replay_inputs = unpack([sample[0] for sample in replay], target, model)
    replay_logits, replay_value, _ = predict(model, replay_inputs, "fp32")
    replay_distribution = torch.distributions.Categorical(
        logits=replay_logits.masked_fill(~replay_inputs[6], -torch.inf)
    )
    reference = torch.distributions.Categorical(
        logits=torch.as_tensor(np.asarray([sample[4] for sample in replay]), device=target)
    )
    action = torch.as_tensor([sample[1] for sample in replay], device=target)
    ratio = (replay_distribution.log_prob(action) - reference.log_prob(action)).exp()
    replay_loss = -.1 * torch.minimum(
        ratio, ratio.clamp(.8, 1.2)
    ).mul((1 - replay_value.sigmoid()).detach()).mean()
    replay_kl = torch.distributions.kl_divergence(reference, replay_distribution).mean()
    assert len(reservoir.rows) == 1 and reservoir.skipped == 1
    assert len(replay) / (9 + len(replay)) <= .1 and replay_loss.isfinite() and replay_kl.isfinite()
    replay[0][4][:] = -20
    replay[0][4][0] = 0
    stale = torch.distributions.Categorical(
        logits=torch.as_tensor(replay[0][4][None], dtype=torch.float32, device=target)
    )
    assert torch.distributions.kl_divergence(stale, replay_distribution).item() > .008
    replay[0][4][:] = np.asarray(replay_distribution.logits[0].detach().cpu(), np.float16)
    refreshed = torch.distributions.Categorical(
        logits=torch.as_tensor(replay[0][4][None], dtype=torch.float32, device=target)
    )
    assert abs(torch.distributions.kl_divergence(refreshed, replay_distribution).item()) < 1e-4
    restored = WinningReservoir(2, 1)
    restored.load_state_dict(reservoir.state_dict())
    assert len(restored.rows) == len(reservoir.rows) and restored.seen == reservoir.seen
    assert np.array_equal(restored.rows[0][0][0], reservoir.rows[0][0][0])
    inputs = tensors(observation, target, model)
    logits, value, progress_value = predict(model, inputs, "fp32")
    (logits.masked_fill(~inputs[6], 0).sum() + value.sum() + progress_value.sum()).backward()
    zero_token_row = list(row)
    zero_token_row[4] = np.zeros((0, 8), np.uint32)
    zero_token_row[5] = np.zeros((0, 24), np.float16)
    zero_token_row[6] = np.zeros(len(row[3]) + 1, np.uint32)
    zero_logits = predict(model, unpack([tuple(zero_token_row)], target, model), "fp32")[0]
    assert zero_logits.isfinite().all()
    permuted = list(observation)
    state = np.array(observation[1], copy=True); state_mask = np.asarray(observation[2], bool)
    action = np.array(observation[4], copy=True); action_mask = np.asarray(observation[5], bool)
    for batch in range(len(state)):
        state[batch, state_mask[batch]] = state[batch, state_mask[batch]][::-1]
        for choice in range(action.shape[1]):
            action[batch, choice, action_mask[batch, choice]] = action[batch, choice, action_mask[batch, choice]][::-1]
    permuted[1] = state; permuted[4] = action
    with torch.no_grad():
        changed = predict(model, tensors(permuted, target, model), "fp32")[0]
    permutation_error = float((logits - changed).abs().max().detach())
    assert permutation_error < 1e-5
    permuted = [np.array(value, copy=True) for value in observation]
    for index in (3, 4, 5, 6, 7):
        permuted[index] = np.flip(permuted[index], axis=1).copy()
    with torch.no_grad():
        reordered = predict(model, tensors(permuted, target, model), "fp32")
    action_permutation_error = max(
        float((logits - reordered[0].flip(1)).abs().max().detach()),
        float((value - reordered[1]).abs().max().detach()),
        float((progress_value - reordered[2]).abs().max().detach()),
    )
    assert action_permutation_error < 1e-5
    candidate = [np.array(value[:1], copy=True) for value in observation]
    candidate[6][0, -1] = 0
    candidate_row = pack(candidate, 0)
    assert len(candidate_row[3]) == len(row[3]) and candidate_row[7].tolist() == [True, True, False]
    assert unpack([candidate_row], target, model)[6].cpu().tolist() == [[True, True, False]]
    first = np.asarray(observation[6], bool).argmax(1)
    env.step(first.tolist())
    parity_inputs = tensors(env.observe_tokens(), target, model)
    with torch.no_grad():
        model.value[-1].weight.normal_(std=.1)
        model.value[-1].bias.fill_(.03)
        python_value = (predict(model, parity_inputs, "fp32")[1] / .83 - .17).sigmoid().cpu().numpy()
    with tempfile.TemporaryDirectory() as directory:
        exported = Path(directory) / "value.bin"
        export_value_model(exported, model, env.fingerprint(), .83, -.17)
        rust = np.asarray(env.rust_values(str(exported)))
    parity_error = float(np.max(np.abs(rust - python_value)))
    assert parity_error <= 1e-5
    cut, step, combat, empty = cuts(
        np.array([False, False, True]), np.array([1, 8, 8]), np.array([1, 2, 8]),
        np.array([True, True, True]), np.array([[True], [False], [False]]), 8, 4,
    )
    assert cut.tolist() == [False, True, False]
    assert step.tolist() == [False, True, False]
    assert combat.tolist() == [False, False, False]
    assert empty.tolist() == [False, True, False]
    one_pass = ExperienceDataset()
    one_pass.rows = list(range(37))
    for key in one_pass.data:
        dtype = one_pass.data[key].dtype
        one_pass.data[key] = (
            np.arange(37, dtype=dtype) if key == "action" else
            np.arange(37, dtype=dtype) % 5 if key == "character" else
            np.ones(37, dtype) if key == "priority" else np.zeros(37, dtype)
        )
    consumed = []
    one_pass_rng = np.random.default_rng(29)
    while len(one_pass):
        selected = one_pass.sample(min(7, len(one_pass)), one_pass_rng)
        consumed.extend(one_pass.data["action"][selected].tolist())
        one_pass.discard(selected)
    assert sorted(consumed) == list(range(37)) and len(consumed) == len(set(consumed))
    completed = ExperienceDataset()
    completed.add({
        "trajectories": [{
            "rows": [row, row], "choices": [0, 0], "old_log": [0, 0],
            "values": [0, 0], "progress_values": [0, 0], "progress_floors": [0, 0],
            "win_rewards": [0, 0], "shaped_rewards": [0, 0],
            "progress_rewards": [0, 0], "terminals": [False, True],
            "characters": [0, 0], "versions": [1, 2], "completion_seconds": 1,
        }, {
            "rows": [row], "choices": [0], "old_log": [0], "values": [0],
            "progress_values": [0], "progress_floors": [0], "win_rewards": [1],
            "shaped_rewards": [1], "progress_rewards": [0], "terminals": [True],
            "characters": [0], "versions": [2], "completion_seconds": 1,
        }],
    }, argparse.Namespace(gae_lambda=1, progress_gamma=1), model)
    assert completed.data["advantage"].tolist() == [0, 0, 1]
    assert completed.data["returns"].tolist() == [0, 0, 1]
    assert completed.data["version"].tolist() == [1, 2, 2]
    balanced = ExperienceDataset(); balanced.rows = list(range(50))
    for key in balanced.data:
        balanced.data[key] = (
            np.repeat(np.arange(5), 10).astype(balanced.data[key].dtype)
            if key == "character" else np.ones(50, balanced.data[key].dtype)
        )
    selected = balanced.sample(25, np.random.default_rng(31))
    assert np.bincount(balanced.data["character"][selected], minlength=5).tolist() == [5] * 5
    assert action_bands([
        (None, None, None, [None] * (actions - 1))
        for actions in (1, 16, 17, 32, 33, 64, 65, 128)
    ]).tolist() == [0, 0, 1, 1, 2, 2, 3, 3]
    banded = ExperienceDataset(); banded.rows = list(range(9))
    for key in banded.data:
        banded.data[key] = (
            np.r_[np.zeros(8), 1].astype(banded.data[key].dtype) if key in ("action_band", "character") else
            np.ones(9, banded.data[key].dtype) if key == "priority" else
            np.zeros(9, banded.data[key].dtype)
        )
    selected = banded.sample(8, np.random.default_rng(32))
    assert len(selected) == 8 and np.all(banded.data["action_band"][selected] == 0)
    collector_args = argparse.Namespace(
        envs=2, training_seed=1_990_000_000, seed=31, max_steps=2048,
        max_combat_steps=512, sampler_steps=1,
    )
    collector = RolloutCollector(collector_args, 0, 0, WinningReservoir(8, 2))
    partial = collector.collect(
        copy.deepcopy(model).cpu().eval(), torch.device("cpu"), "fp32",
        time.monotonic() + 30, 1, 0,
    )
    assert not partial["trajectories"] and all(trajectory is not None for trajectory in collector.trajectories)
    collector_args.max_steps = 1
    collector = RolloutCollector(collector_args, 1, 0, WinningReservoir(8, 2))
    truncated = collector.collect(
        copy.deepcopy(model).cpu().eval(), torch.device("cpu"), "fp32",
        time.monotonic() + 30, 1, 0,
    )
    assert not truncated["trajectories"] and all(trajectory is None for trajectory in collector.trajectories)
    assert truncated["discarded_steps"] == 2
    assert sum(len(rows) for rows in truncated["episodes"]) == 2
    produced = 0
    while produced < 96:
        produced += sampling_steps(produced, 96, 8, 5) * 8
    assert produced == 96
    scaled = []
    for name, config, expected in (
        ("wide", (224, 2, 4, 448), 4_971_907),
        ("deep", (160, 13, 4, 320), 4_953_123),
    ):
        candidate = Agent(layout, *config).to(target)
        optimizer = torch.optim.Adam(candidate.parameters(), lr=3e-4, eps=1e-5, fused=target.type != "cpu")
        candidate.train()
        with torch.no_grad():
            reference = predict(candidate, inputs, "fp32")
        precision = "bf16" if target.type in ("mps", "cuda") else "fp32"
        output = predict(candidate, inputs, precision)
        legal = inputs[6]
        action = legal.float().argmax(1)
        distribution = torch.distributions.Categorical(logits=output[0].masked_fill(~legal, -torch.inf))
        loss = -distribution.log_prob(action).mean() + nn.functional.binary_cross_entropy_with_logits(
            output[1], torch.zeros_like(output[1])
        ) + output[2].square().mean()
        reference_loss = -torch.distributions.Categorical(
            logits=reference[0].masked_fill(~legal, -torch.inf)
        ).log_prob(action).mean() + nn.functional.binary_cross_entropy_with_logits(
            reference[1], torch.zeros_like(reference[1])
        ) + reference[2].square().mean()
        error = max(float((left - right).abs().max().detach()) for left, right in zip(reference, output))
        loss_error = abs(float(reference_loss - loss.detach()))
        assert error < .05 and loss_error < .05 and loss.isfinite()
        optimizer.zero_grad(set_to_none=True); loss.backward()
        gradients = [parameter.grad for parameter in candidate.parameters() if parameter.grad is not None]
        assert gradients and all(gradient.isfinite().all() for gradient in gradients)
        optimizer.step()
        assert sum(parameter.numel() for parameter in candidate.parameters()) == expected
        assert all(parameter.dtype == torch.float32 and parameter.isfinite().all() for parameter in candidate.parameters())
        assert all(
            not torch.is_tensor(value) or not value.is_floating_point() or value.dtype == torch.float32
            for state in optimizer.state.values() for value in state.values()
        )
        if target.type == "mps":
            torch.mps.synchronize()
            memory = {"allocated": torch.mps.current_allocated_memory(), "driver": torch.mps.driver_allocated_memory()}
        elif target.type == "cuda":
            torch.cuda.synchronize()
            memory = {"allocated": torch.cuda.memory_allocated(), "peak": torch.cuda.max_memory_allocated()}
        else:
            memory = {}
        scaled.append({"name": name, "architecture": config, "parameters": expected,
                       "precision": precision, "output_error": error, "loss_error": loss_error,
                       "loss": float(loss.detach()), "memory_bytes": memory})
        del candidate, optimizer
    print(json.dumps({
        "device": str(target), "layout": layout,
        "shapes": [list(np.asarray(value).shape) for value in observation],
        "parameters": sum(parameter.numel() for parameter in model.parameters()),
        "permutation_error": permutation_error,
        "action_permutation_error": action_permutation_error, "rust_parity_error": parity_error,
        "winning_reservoir": len(reservoir.rows),
        "one_pass_rows": len(consumed), "continuous_decisions": produced, "scaled": scaled,
    }))


def parser():
    root = argparse.ArgumentParser()
    commands = root.add_subparsers(dest="command", required=True)
    run = commands.add_parser("train")
    run.add_argument("--output", default="target/v38")
    run.add_argument("--checkpoint")
    run.add_argument("--width", type=int)
    run.add_argument("--layers", type=int)
    run.add_argument("--heads", type=int)
    run.add_argument("--feedforward", type=int)
    run.add_argument("--precision", choices=("fp32", "bf16"), default="bf16")
    run.add_argument("--hours", type=float, default=0)
    run.add_argument("--decisions", type=int, default=0)
    run.add_argument("--envs", type=int, default=128)
    run.add_argument("--sampler-steps", type=int, default=4)
    run.add_argument("--report-decisions", type=int, default=32_768)
    run.add_argument("--save-decisions", type=int, default=262_144)
    run.add_argument("--max-policy-lag", type=int, default=128)
    run.add_argument("--max-log-ratio", type=float, default=.5)
    run.add_argument("--batch", type=int, default=1024)
    run.add_argument("--learning-rate", type=float, default=3e-4)
    run.add_argument("--gae-lambda", type=float, default=1.0)
    run.add_argument("--progress-gamma", type=float, default=.99)
    run.add_argument("--progress-decisions", type=int, default=5_000_000)
    run.add_argument("--clip", type=float, default=0.2)
    run.add_argument("--value-weight", type=float, default=0.5)
    run.add_argument("--entropy-start", type=float, default=0.01)
    run.add_argument("--entropy-end", type=float, default=0.001)
    run.add_argument("--target-kl", type=float, default=0.008)
    run.add_argument("--training-seed", type=int, default=1_900_000_000)
    run.add_argument("--promotion-seed", type=int, default=3_700_000_000)
    run.add_argument("--promotion-window", type=int, default=32)
    run.add_argument("--promotion-runs", type=int, default=128)
    run.add_argument("--promote-win-rate", type=float, default=0.2)
    run.add_argument("--evaluation-batch", type=int, default=32)
    run.add_argument("--evaluation-max-steps", type=int, default=2048)
    run.add_argument("--evaluation-max-combat-steps", type=int, default=512)
    run.add_argument("--max-steps", type=int, default=2048)
    run.add_argument("--max-combat-steps", type=int, default=512)
    run.add_argument("--seed", type=int, default=1)
    evaluate_parser = commands.add_parser("evaluate")
    evaluate_parser.add_argument("checkpoint")
    evaluate_parser.add_argument("--ascension", type=int, default=0)
    evaluate_parser.add_argument("--bonus", type=int, default=24)
    evaluate_parser.add_argument("--validation-seed", type=int, default=3_500_000_000)
    evaluate_parser.add_argument("--runs", type=int, default=128)
    evaluate_parser.add_argument("--evaluation-batch", type=int, default=32)
    evaluate_parser.add_argument("--max-steps", type=int, default=2048)
    evaluate_parser.add_argument("--max-combat-steps", type=int, default=512)
    evaluate_parser.add_argument("--precision", choices=("fp32", "bf16"), default="fp32")
    final = commands.add_parser("finalize")
    final.add_argument("checkpoint")
    final.add_argument("--output", default="target/value/value.bin")
    final.add_argument("--fit-runs", type=int, default=256)
    final.add_argument("--calibration-runs", type=int, default=256)
    final.add_argument("--test-runs", type=int, default=1000)
    final.add_argument("--states-per-run", type=int, default=16)
    final.add_argument("--hours", type=float, default=12)
    final.add_argument("--batch", type=int, default=32)
    final.add_argument("--head-batch", type=int, default=1024)
    final.add_argument("--epochs", type=int, default=8)
    final.add_argument("--learning-rate", type=float, default=3e-4)
    final.add_argument("--fit-seed", type=int, default=4_100_000_000)
    final.add_argument("--calibration-seed", type=int, default=4_200_000_000)
    final.add_argument("--test-seed", type=int, default=4_300_000_000)
    final.add_argument("--max-steps", type=int, default=4096)
    final.add_argument("--max-combat-steps", type=int, default=512)
    final.add_argument("--precision", choices=("fp32", "bf16"))
    final.add_argument("--seed", type=int, default=29)
    dashboard_parser = commands.add_parser("dashboard")
    dashboard_parser.add_argument("target", nargs="?", default="target")
    commands.add_parser("probe")
    return root


if __name__ == "__main__":
    args = parser().parse_args()
    if args.command == "train":
        train(args)
    elif args.command == "finalize":
        finalize(args)
    elif args.command == "evaluate":
        target = device(); model, _ = load(args.checkpoint, target)
        print(json.dumps(evaluate(model, args, target), indent=2))
    elif args.command == "dashboard":
        dashboard(Path(args.target))
    else:
        probe()
