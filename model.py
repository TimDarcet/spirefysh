import fcntl
import hashlib
import json
import math
import os
import struct
import tempfile
from pathlib import Path

import numpy as np
import torch
from torch import nn

import sts2_sim

FEATURE_VERSION, MODEL_VERSION, CATEGORIES, MAX_PROGRESS, ACTION_FIELDS, TOKEN_SPECS, \
    SEMANTIC_NAMES, POSITION_CAPS = sts2_sim.model_schema()
ACTION_FIELDS = tuple(ACTION_FIELDS)
TOKEN_SPECS = tuple(tuple(row) for row in TOKEN_SPECS)
SEMANTIC_NAMES = tuple(SEMANTIC_NAMES)
POSITION_CAPS = dict(POSITION_CAPS)
COMPATIBLE_MODEL_VERSIONS = (MODEL_VERSION,)
DEFAULT_ARCHITECTURE = (128, 4, 8, 384)
PRECISIONS = ("fp32", "bf16")
WINNING_CAPACITY = 0
CHANGE = "Current-floor plus resource-potential residual critic."
COLLECTION_POOLING = ("sum", "transformer", "global_tokens")
EFFECT_POOLING = (
    "sum_into_actor", "transformer_into_actor", "sum_token", "transformer_token",
    "global_tokens",
)
CONTINUATION_POOLING = ("sum", "transformer", "gru", "global_tokens")
POOLING_DEFAULTS = {
    "relic": "global_tokens", "deck": "global_tokens", "draw": "global_tokens",
    "exhaust": "global_tokens", "discard": "global_tokens", "hand": "global_tokens",
    "orb": "global_tokens", "potion": "global_tokens",
    "enemy_effect": "global_tokens", "friendly_effect": "global_tokens",
    "continuation": "gru", "phase": "transformer", "generation_pool": "sum",
}
STAGES = [(0, bonus) for bonus in (24, 20, 16, 12, 8, 4, 3, 2, 1, 0)] + [
    (ascension, 0) for ascension in range(1, 11)
]
POTENTIAL_TERMS = (
    "bias", "max_hp", "upgrades", "relics", "potions", "gold", "cards",
    "uncommon_relics", "rare_relics", "uncommon_potions", "rare_potions",
    "uncommon_cards", "rare_cards",
)
OPTIMIZER_CONFIG = {"type": "Adam", "betas": (.9, .99), "eps": 1e-5}

_ACCELERATOR_LOCK = None
_TRAINING_METAL = r"""
#include <metal_stdlib>
using namespace metal;
constant uint SIMD_WIDTH = 32;

inline float head_sum(float value, uint dimension, uint lane) {
    if (dimension == SIMD_WIDTH) return simd_sum(value);
    float prefix = simd_prefix_inclusive_sum(value);
    float low = simd_shuffle(prefix, dimension - 1);
    float high = simd_shuffle(prefix, SIMD_WIDTH - 1) - low;
    return lane < dimension ? low : high;
}

kernel void attention_forward(
    device const float *qkv, device const int *offsets, device const int *sequence,
    device const int *selected,
    device float *output, device float *lse, constant uint& heads, constant uint& dimension,
    constant bool& sparse,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, query_index = group / groups;
    uint token = sparse ? selected[query_index] : query_index;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = token * 3 * width + head * dimension + column;
    if (end == begin + 1) {
        output[query_index * width + head * dimension + column] = qkv[query + 2 * width];
        if (column == 0) lse[query_index * heads + head] = 0.0f;
        return;
    }
    float q = qkv[query];
    float maximum = -INFINITY, sum = 0.0f, value = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 3 * width + width + head * dimension;
        float score = head_sum(q * qkv[k + column], dimension, lane) * rsqrt(float(dimension));
        float next = max(maximum, score), old = exp(maximum - next), weight = exp(score - next);
        sum = sum * old + weight;
        value = value * old + weight * qkv[k + width + column];
        maximum = next;
    }
    output[query_index * width + head * dimension + column] = value / sum;
    if (column == 0) lse[query_index * heads + head] = maximum + log(sum);
}

kernel void backward_query(
    device const float *qkv, device const int *offsets, device const int *sequence,
    device const int *selected,
    device const float *output, device const float *lse, device const float *grad_output,
    device float *grad_qkv, device float *delta, constant uint& heads, constant uint& dimension,
    constant bool& sparse,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, query_index = group / groups;
    uint token = sparse ? selected[query_index] : query_index;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = token * 3 * width + head * dimension + column;
    uint out = query_index * width + head * dimension + column;
    if (end == begin + 1) {
        grad_qkv[query] = 0.0f;
        if (column == 0) delta[query_index * heads + head] = 0.0f;
        return;
    }
    float q = qkv[query], grad = grad_output[out];
    float correction = head_sum(grad * output[out], dimension, lane), dq = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 3 * width + width + head * dimension;
        float key_value = qkv[k + column];
        float probability = exp(head_sum(q * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[query_index * heads + head]);
        float dp = head_sum(grad * qkv[k + width + column], dimension, lane);
        dq += probability * (dp - correction) * key_value * rsqrt(float(dimension));
    }
    grad_qkv[query] = dq;
    if (column == 0) delta[query_index * heads + head] = correction;
}

kernel void backward_key_value(
    device const float *qkv, device const int *offsets, device const int *sequence,
    device const int *selected, device const int *query_offsets,
    device const float *output, device const float *lse, device const float *grad_output,
    device const float *delta, device float *grad_qkv, constant uint& heads, constant uint& dimension,
    constant bool& sparse,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, key = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[key], begin = sparse ? query_offsets[row] : offsets[row];
    int end = sparse ? query_offsets[row + 1] : offsets[row + 1];
    uint width = heads * dimension, k = key * 3 * width + width + head * dimension;
    if (offsets[row + 1] == offsets[row] + 1) {
        grad_qkv[k + column] = 0.0f;
        grad_qkv[k + width + column] = grad_output[begin * width + head * dimension + column];
        return;
    }
    bool selected_query = false;
    float value = qkv[k + width + column];
    float dk = 0.0f, dv = 0.0f;
    for (int query_index = begin; query_index < end; ++query_index) {
        uint token = sparse ? selected[query_index] : query_index;
        if (sparse) selected_query |= token == key;
        uint query = token * 3 * width + head * dimension + column;
        uint out = query_index * width + head * dimension + column;
        float q = qkv[query], key_value = qkv[k + column], grad = grad_output[out];
        float probability = exp(head_sum(q * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[query_index * heads + head]);
        float ds = probability * (head_sum(grad * value, dimension, lane) - delta[query_index * heads + head]);
        dk += ds * q * rsqrt(float(dimension));
        dv += probability * grad;
    }
    if (sparse && !selected_query) grad_qkv[key * 3 * width + head * dimension + column] = 0.0f;
    grad_qkv[k + column] = dk;
    grad_qkv[k + width + column] = dv;
}

kernel void attention_forward_bfloat(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device const int *selected,
    device bfloat *output, device float *lse, constant uint& heads, constant uint& dimension,
    constant bool& sparse,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, query_index = group / groups;
    uint token = sparse ? selected[query_index] : query_index;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = token * 3 * width + head * dimension + column;
    if (end == begin + 1) {
        output[query_index * width + head * dimension + column] = qkv[query + 2 * width];
        if (column == 0) lse[query_index * heads + head] = 0.0f;
        return;
    }
    float q = float(qkv[query]);
    float maximum = -INFINITY, sum = 0.0f, value = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 3 * width + width + head * dimension;
        float score = head_sum(q * float(qkv[k + column]), dimension, lane) * rsqrt(float(dimension));
        float next = max(maximum, score), old = exp(maximum - next), weight = exp(score - next);
        sum = sum * old + weight;
        value = value * old + weight * float(qkv[k + width + column]);
        maximum = next;
    }
    output[query_index * width + head * dimension + column] = bfloat(value / sum);
    if (column == 0) lse[query_index * heads + head] = maximum + log(sum);
}

kernel void attention_backward_query_bfloat(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device const int *selected,
    device const bfloat *output, device const float *lse, device const bfloat *grad_output,
    device bfloat *grad_qkv, device float *delta, constant uint& heads, constant uint& dimension,
    constant bool& sparse,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, query_index = group / groups;
    uint token = sparse ? selected[query_index] : query_index;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = token * 3 * width + head * dimension + column;
    uint out = query_index * width + head * dimension + column;
    if (end == begin + 1) {
        grad_qkv[query] = bfloat(0.0f);
        if (column == 0) delta[query_index * heads + head] = 0.0f;
        return;
    }
    float q = float(qkv[query]), grad = float(grad_output[out]);
    float correction = head_sum(grad * float(output[out]), dimension, lane), dq = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 3 * width + width + head * dimension;
        float key_value = float(qkv[k + column]);
        float probability = exp(head_sum(q * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[query_index * heads + head]);
        float dp = head_sum(grad * float(qkv[k + width + column]), dimension, lane);
        dq += probability * (dp - correction) * key_value * rsqrt(float(dimension));
    }
    grad_qkv[query] = bfloat(dq);
    if (column == 0) delta[query_index * heads + head] = correction;
}

kernel void attention_backward_key_value_bfloat(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device const int *selected, device const int *query_offsets,
    device const bfloat *output, device const float *lse, device const bfloat *grad_output,
    device const float *delta, device bfloat *grad_qkv, constant uint& heads, constant uint& dimension,
    constant bool& sparse,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, key = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[key], begin = sparse ? query_offsets[row] : offsets[row];
    int end = sparse ? query_offsets[row + 1] : offsets[row + 1];
    uint width = heads * dimension, k = key * 3 * width + width + head * dimension;
    if (offsets[row + 1] == offsets[row] + 1) {
        grad_qkv[k + column] = bfloat(0.0f);
        grad_qkv[k + width + column] = grad_output[begin * width + head * dimension + column];
        return;
    }
    bool selected_query = false;
    float value = float(qkv[k + width + column]);
    float dk = 0.0f, dv = 0.0f;
    for (int query_index = begin; query_index < end; ++query_index) {
        uint token = sparse ? selected[query_index] : query_index;
        if (sparse) selected_query |= token == key;
        uint query = token * 3 * width + head * dimension + column;
        uint out = query_index * width + head * dimension + column;
        float q = float(qkv[query]), key_value = float(qkv[k + column]), grad = float(grad_output[out]);
        float probability = exp(head_sum(q * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[query_index * heads + head]);
        float ds = probability * (head_sum(grad * value, dimension, lane) - delta[query_index * heads + head]);
        dk += ds * q * rsqrt(float(dimension));
        dv += probability * grad;
    }
    if (sparse && !selected_query)
        grad_qkv[key * 3 * width + head * dimension + column] = bfloat(0.0f);
    grad_qkv[k + column] = bfloat(dk);
    grad_qkv[k + width + column] = bfloat(dv);
}

kernel void summary_forward(
    device const float *q, device const float *kv, device const int *offsets,
    device float *output, device float *lse, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, row = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = row * width + head * dimension + column;
    if (begin == end) {
        output[query] = 0.0f;
        if (column == 0) lse[row * heads + head] = 0.0f;
        return;
    }
    float query_value = q[query], maximum = -INFINITY, sum = 0.0f, value = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 2 * width + head * dimension;
        float score = head_sum(query_value * kv[k + column], dimension, lane) * rsqrt(float(dimension));
        float next = max(maximum, score), old = exp(maximum - next), weight = exp(score - next);
        sum = sum * old + weight;
        value = value * old + weight * kv[k + width + column];
        maximum = next;
    }
    output[query] = value / sum;
    if (column == 0) lse[row * heads + head] = maximum + log(sum);
}

kernel void summary_backward_query(
    device const float *q, device const float *kv, device const int *offsets,
    device const float *output, device const float *lse, device const float *grad_output,
    device float *grad_q, device float *delta, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, row = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = row * width + head * dimension + column;
    if (begin == end) {
        grad_q[query] = 0.0f;
        if (column == 0) delta[row * heads + head] = 0.0f;
        return;
    }
    float query_value = q[query], grad = grad_output[query];
    float correction = head_sum(grad * output[query], dimension, lane), dq = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 2 * width + head * dimension;
        float key_value = kv[k + column];
        float probability = exp(head_sum(query_value * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[row * heads + head]);
        float dp = head_sum(grad * kv[k + width + column], dimension, lane);
        dq += probability * (dp - correction) * key_value * rsqrt(float(dimension));
    }
    grad_q[query] = dq;
    if (column == 0) delta[row * heads + head] = correction;
}

kernel void summary_backward_key_value(
    device const float *q, device const float *kv, device const int *sequence,
    device const float *lse, device const float *grad_output, device const float *delta,
    device float *grad_kv, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, key = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[key];
    uint width = heads * dimension, query = row * width + head * dimension + column;
    uint k = key * 2 * width + head * dimension;
    float query_value = q[query], grad = grad_output[query], value = kv[k + width + column];
    float probability = exp(head_sum(query_value * kv[k + column], dimension, lane) * rsqrt(float(dimension)) - lse[row * heads + head]);
    float ds = probability * (head_sum(grad * value, dimension, lane) - delta[row * heads + head]);
    grad_kv[k + column] = ds * query_value * rsqrt(float(dimension));
    grad_kv[k + width + column] = probability * grad;
}

kernel void summary_backward(
    device const float *q, device const float *kv, device const int *offsets,
    device const float *output, device const float *lse, device const float *grad_output,
    device float *grad_q, device float *grad_kv, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, row = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = row * width + head * dimension + column;
    if (begin == end) {
        grad_q[query] = 0.0f;
        return;
    }
    float query_value = q[query], grad = grad_output[query];
    float correction = head_sum(grad * output[query], dimension, lane), dq = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 2 * width + head * dimension;
        float key_value = kv[k + column], value = kv[k + width + column];
        float probability = exp(head_sum(query_value * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[row * heads + head]);
        float ds = probability * (head_sum(grad * value, dimension, lane) - correction);
        dq += ds * key_value * rsqrt(float(dimension));
        grad_kv[k + column] = ds * query_value * rsqrt(float(dimension));
        grad_kv[k + width + column] = probability * grad;
    }
    grad_q[query] = dq;
}

kernel void summary_forward_bfloat(
    device const bfloat *q, device const bfloat *kv, device const int *offsets,
    device bfloat *output, device float *lse, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, row = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = row * width + head * dimension + column;
    if (begin == end) {
        output[query] = bfloat(0.0f);
        if (column == 0) lse[row * heads + head] = 0.0f;
        return;
    }
    float query_value = float(q[query]), maximum = -INFINITY, sum = 0.0f, value = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 2 * width + head * dimension;
        float score = head_sum(query_value * float(kv[k + column]), dimension, lane) * rsqrt(float(dimension));
        float next = max(maximum, score), old = exp(maximum - next), weight = exp(score - next);
        sum = sum * old + weight;
        value = value * old + weight * float(kv[k + width + column]);
        maximum = next;
    }
    output[query] = bfloat(value / sum);
    if (column == 0) lse[row * heads + head] = maximum + log(sum);
}

kernel void summary_backward_query_bfloat(
    device const bfloat *q, device const bfloat *kv, device const int *offsets,
    device const bfloat *output, device const float *lse, device const bfloat *grad_output,
    device bfloat *grad_q, device float *delta, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, row = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = row * width + head * dimension + column;
    if (begin == end) {
        grad_q[query] = bfloat(0.0f);
        if (column == 0) delta[row * heads + head] = 0.0f;
        return;
    }
    float query_value = float(q[query]), grad = float(grad_output[query]);
    float correction = head_sum(grad * float(output[query]), dimension, lane), dq = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 2 * width + head * dimension;
        float key_value = float(kv[k + column]);
        float probability = exp(head_sum(query_value * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[row * heads + head]);
        float dp = head_sum(grad * float(kv[k + width + column]), dimension, lane);
        dq += probability * (dp - correction) * key_value * rsqrt(float(dimension));
    }
    grad_q[query] = bfloat(dq);
    if (column == 0) delta[row * heads + head] = correction;
}

kernel void summary_backward_key_value_bfloat(
    device const bfloat *q, device const bfloat *kv, device const int *sequence,
    device const float *lse, device const bfloat *grad_output, device const float *delta,
    device bfloat *grad_kv, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, key = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[key];
    uint width = heads * dimension, query = row * width + head * dimension + column;
    uint k = key * 2 * width + head * dimension;
    float query_value = float(q[query]), grad = float(grad_output[query]);
    float value = float(kv[k + width + column]);
    float probability = exp(head_sum(query_value * float(kv[k + column]), dimension, lane) * rsqrt(float(dimension)) - lse[row * heads + head]);
    float ds = probability * (head_sum(grad * value, dimension, lane) - delta[row * heads + head]);
    grad_kv[k + column] = bfloat(ds * query_value * rsqrt(float(dimension)));
    grad_kv[k + width + column] = bfloat(probability * grad);
}

kernel void summary_backward_bfloat(
    device const bfloat *q, device const bfloat *kv, device const int *offsets,
    device const bfloat *output, device const float *lse, device const bfloat *grad_output,
    device bfloat *grad_q, device bfloat *grad_kv, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, row = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = row * width + head * dimension + column;
    if (begin == end) {
        grad_q[query] = bfloat(0.0f);
        return;
    }
    float query_value = float(q[query]), grad = float(grad_output[query]);
    float correction = head_sum(grad * float(output[query]), dimension, lane), dq = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 2 * width + head * dimension;
        float key_value = float(kv[k + column]), value = float(kv[k + width + column]);
        float probability = exp(head_sum(query_value * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[row * heads + head]);
        float ds = probability * (head_sum(grad * value, dimension, lane) - correction);
        dq += ds * key_value * rsqrt(float(dimension));
        grad_kv[k + column] = bfloat(ds * query_value * rsqrt(float(dimension)));
        grad_kv[k + width + column] = bfloat(probability * grad);
    }
    grad_q[query] = bfloat(dq);
}


kernel void ragged_log_softmax(
    device const float *score, device const int *offsets, device float *output,
    constant uint& rows, uint row [[thread_position_in_grid]]) {
    int begin = offsets[row], end = offsets[row + 1];
    float maximum = -INFINITY;
    for (int index = begin; index < end; ++index) maximum = max(maximum, score[index]);
    float sum = 0.0f;
    for (int index = begin; index < end; ++index) sum += exp(score[index] - maximum);
    float normalizer = maximum + log(sum);
    for (int index = begin; index < end; ++index)
        output[index] = isfinite(score[index]) ? score[index] - normalizer : 0.0f;
}

kernel void ragged_log_softmax_bfloat(
    device const bfloat *score, device const int *offsets, device bfloat *output,
    constant uint& rows, uint row [[thread_position_in_grid]]) {
    int begin = offsets[row], end = offsets[row + 1];
    float maximum = -INFINITY;
    for (int index = begin; index < end; ++index) maximum = max(maximum, float(score[index]));
    float sum = 0.0f;
    for (int index = begin; index < end; ++index) sum += exp(float(score[index]) - maximum);
    float normalizer = maximum + log(sum);
    for (int index = begin; index < end; ++index)
        output[index] = isfinite(float(score[index])) ? bfloat(float(score[index]) - normalizer) : bfloat(0.0f);
}

kernel void ragged_log_softmax_backward(
    device const float *score, device const float *output, device const float *grad_output,
    device const int *offsets, device float *grad_score,
    constant uint& rows, uint row [[thread_position_in_grid]]) {
    int begin = offsets[row], end = offsets[row + 1];
    float sum = 0.0f;
    for (int index = begin; index < end; ++index)
        if (isfinite(score[index])) sum += grad_output[index];
    for (int index = begin; index < end; ++index)
        grad_score[index] = isfinite(score[index]) ? grad_output[index] - exp(output[index]) * sum : 0.0f;
}

kernel void ragged_log_softmax_backward_bfloat(
    device const bfloat *score, device const bfloat *output, device const bfloat *grad_output,
    device const int *offsets, device bfloat *grad_score,
    constant uint& rows, uint row [[thread_position_in_grid]]) {
    int begin = offsets[row], end = offsets[row + 1];
    float sum = 0.0f;
    for (int index = begin; index < end; ++index)
        if (isfinite(float(score[index]))) sum += float(grad_output[index]);
    for (int index = begin; index < end; ++index)
        grad_score[index] = isfinite(float(score[index]))
            ? bfloat(float(grad_output[index]) - exp(float(output[index])) * sum) : bfloat(0.0f);
}

"""


class _RaggedAttention(torch.autograd.Function):
    library = None
    half_library = None

    @staticmethod
    def forward(ctx, qkv, offsets, sequence, heads, selected, query_offsets, sparse):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        if qkv.dtype == torch.float16 and _RaggedAttention.half_library is None:
            _RaggedAttention.half_library = torch.mps.compile_shader(
                _TRAINING_METAL.replace("bfloat", "half")
            )
        library = _RaggedAttention.half_library if qkv.dtype == torch.float16 else _RaggedAttention.library
        queries = len(selected) if sparse else len(qkv)
        output = qkv.new_empty((queries, qkv.shape[1] // 3))
        lse = torch.empty((queries, heads), dtype=torch.float32, device=qkv.device)
        dimension = output.shape[1] // heads
        threads = queries * heads * dimension
        suffix = "_half" if qkv.dtype == torch.float16 else "_bfloat" if qkv.dtype == torch.bfloat16 else ""
        kernel = getattr(library, f"attention_forward{suffix}")
        kernel(
            qkv, offsets, sequence, selected, output, lse, heads, dimension, sparse,
            threads=threads, group_size=32,
        )
        ctx.save_for_backward(qkv, offsets, sequence, selected, query_offsets, output, lse)
        ctx.heads = heads
        ctx.dimension = dimension
        ctx.library, ctx.suffix, ctx.sparse = library, suffix, sparse
        return output

    @staticmethod
    def backward(ctx, grad_output):
        qkv, offsets, sequence, selected, query_offsets, output, lse = ctx.saved_tensors
        grad_qkv = torch.empty_like(qkv)
        delta = torch.empty_like(lse)
        query_threads = len(output) * ctx.heads * ctx.dimension
        key_threads = len(qkv) * ctx.heads * ctx.dimension
        args = qkv, offsets, sequence, selected, output, lse, grad_output.contiguous()
        query = getattr(ctx.library, f"attention_backward_query{ctx.suffix}") if ctx.suffix else ctx.library.backward_query
        key_value = getattr(ctx.library, f"attention_backward_key_value{ctx.suffix}") if ctx.suffix else ctx.library.backward_key_value
        query(
            *args, grad_qkv, delta, ctx.heads, ctx.dimension, ctx.sparse,
            threads=query_threads, group_size=32,
        )
        key_value(
            qkv, offsets, sequence, selected, query_offsets, output, lse,
            grad_output.contiguous(), delta, grad_qkv, ctx.heads, ctx.dimension, ctx.sparse,
            threads=key_threads, group_size=32,
        )
        return grad_qkv, None, None, None, None, None, None


class _RaggedSummaryAttention(torch.autograd.Function):
    @staticmethod
    def forward(ctx, query, key_value, offsets, sequence, heads):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        if query.dtype == torch.float16 and _RaggedAttention.half_library is None:
            _RaggedAttention.half_library = torch.mps.compile_shader(
                _TRAINING_METAL.replace("bfloat", "half")
            )
        library = _RaggedAttention.half_library if query.dtype == torch.float16 else _RaggedAttention.library
        output = torch.empty_like(query)
        lse = torch.empty((len(query), heads), dtype=torch.float32, device=query.device)
        dimension = query.shape[1] // heads
        suffix = "_half" if query.dtype == torch.float16 else "_bfloat" if query.dtype == torch.bfloat16 else ""
        getattr(library, f"summary_forward{suffix}")(
            query, key_value, offsets, output, lse, heads, dimension,
            threads=len(query) * heads * dimension, group_size=32,
        )
        ctx.save_for_backward(query, key_value, offsets, sequence, output, lse)
        ctx.heads, ctx.dimension, ctx.library, ctx.suffix = heads, dimension, library, suffix
        return output

    @staticmethod
    def backward(ctx, grad_output):
        query, key_value, offsets, sequence, output, lse = ctx.saved_tensors
        grad_query, grad_key_value = torch.empty_like(query), torch.empty_like(key_value)
        grad_output = grad_output.contiguous()
        threads = len(query) * ctx.heads * ctx.dimension
        getattr(ctx.library, f"summary_backward{ctx.suffix}")(
            query, key_value, offsets, output, lse, grad_output, grad_query, grad_key_value,
            ctx.heads, ctx.dimension, threads=threads, group_size=32,
        )
        return grad_query, grad_key_value, None, None, None


class _RaggedLogSoftmax(torch.autograd.Function):
    @staticmethod
    def forward(ctx, score, offsets):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        library = _RaggedAttention.library
        suffix = "_bfloat" if score.dtype == torch.bfloat16 else ""
        output = torch.empty_like(score)
        rows = len(offsets) - 1
        getattr(library, f"ragged_log_softmax{suffix}")(
            score, offsets, output, rows, threads=rows, group_size=256,
        )
        ctx.save_for_backward(score, offsets, output)
        ctx.library, ctx.suffix, ctx.rows = library, suffix, rows
        return output

    @staticmethod
    def backward(ctx, grad_output):
        score, offsets, output = ctx.saved_tensors
        grad_score = torch.empty_like(score)
        getattr(ctx.library, f"ragged_log_softmax_backward{ctx.suffix}")(
            score, output, grad_output.contiguous(), offsets, grad_score, ctx.rows,
            threads=ctx.rows, group_size=256,
        )
        return grad_score, None



def device():
    global _ACCELERATOR_LOCK
    if torch.backends.mps.is_available():
        if _ACCELERATOR_LOCK is None:
            path = Path(tempfile.gettempdir()) / f"spirefysh-{os.getuid()}-mps.lock"
            lock = path.open("a+")
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                lock.seek(0)
                owner = lock.read().strip() or "unknown"
                lock.close()
                raise RuntimeError(f"MPS is already in use by process {owner}") from error
            lock.seek(0); lock.truncate(); lock.write(str(os.getpid())); lock.flush()
            _ACCELERATOR_LOCK = lock
        return torch.device("mps")
    if torch.cuda.is_available():
        return torch.device("cuda")
    return torch.device("cpu")


def release_accelerator():
    global _ACCELERATOR_LOCK
    if _ACCELERATOR_LOCK is not None:
        _ACCELERATOR_LOCK.close()
        _ACCELERATOR_LOCK = None


DOMAIN = {name: index for index, (name, *_shape) in enumerate(TOKEN_SPECS)}


class ConceptEmbeddings(nn.Module):
    def __init__(self, layout, width):
        super().__init__()
        self.starts = tuple(layout[name + "_semantic_start"] for name in SEMANTIC_NAMES)
        self.sizes = tuple(layout[name + "_semantic_count"] for name in SEMANTIC_NAMES)
        self.tables = nn.ModuleList(nn.Embedding(size + 1, width, padding_idx=0)
                                    for size in self.sizes)
        self.num_embeddings = layout["concept_vocab"]

    def forward(self, codes):
        return nn.functional.embedding(codes, self.flattened())

    def flattened(self):
        return torch.cat((self.tables[0].weight[:1] * 0,
                          *(table.weight[1:] for table in self.tables)))

    def local(self, name, index):
        return self.tables[SEMANTIC_NAMES.index(name)].weight[index + 1]


class TokenEncoder(nn.Module):
    def __init__(self, _categorical, numeric, width):
        super().__init__()
        self.numeric = nn.Linear(numeric, width, bias=False)
        self.norm = nn.LayerNorm(width)

    def forward(self, categorical, numeric, embeddings):
        if not len(categorical):
            return self.norm.weight.new_empty((0, len(self.norm.weight)))
        return self.norm(nn.functional.embedding(categorical.long(), embeddings).sum(1)
                         + self.numeric(numeric))


class Agent(nn.Module):
    def __init__(self, layout, width=128, layers=4, heads=8, feedforward=384, head_width=None,
                 pooling=None, model_version=MODEL_VERSION, potential_weights=None):
        nn.Module.__init__(self)
        self.layout = dict(layout)
        if self.layout["version"] != FEATURE_VERSION:
            raise ValueError("incompatible observation layout")
        expected = {name + suffix: value for name, unsigned, signed, semantic, numeric in TOKEN_SPECS
                    for suffix, value in (("_u", unsigned), ("_s", signed),
                                          ("_c", semantic), ("_f", numeric))}
        expected |= {"domain_count": len(TOKEN_SPECS), "action_u": ACTION_FIELDS[0],
                     "action_s": ACTION_FIELDS[1], "action_c": ACTION_FIELDS[2],
                     "action_f": ACTION_FIELDS[3], "globals": 0, "model_width": width,
                     "model_layers": layers, "model_heads": heads,
                     "model_feedforward": feedforward, "action_width": width}
        if any(self.layout.get(key) != value for key, value in expected.items()):
            raise ValueError("incompatible observation schema")
        if min(width, layers, heads, feedforward) <= 0 or width % heads \
                or head_width not in (None, width) \
                or model_version not in COMPATIBLE_MODEL_VERSIONS:
            raise ValueError("invalid architecture")
        self.model_version = model_version
        self.width, self.layers, self.heads, self.feedforward = width, layers, heads, feedforward
        self.head_width = width
        self.state_width = width
        self.potential_weights = tuple(potential_weights or (0.,) * len(POTENTIAL_TERMS))
        if len(self.potential_weights) != len(POTENTIAL_TERMS) \
                or not all(map(math.isfinite, self.potential_weights)):
            raise ValueError("invalid potential weights")
        self.card_zones = 5
        self.entity_collections = 0
        self.pooling = POOLING_DEFAULTS | (pooling or {})
        for name in ("relic", "deck", "draw", "exhaust", "discard", "hand", "orb", "potion"):
            if self.pooling[name] not in COLLECTION_POOLING:
                raise ValueError(f"invalid {name} pooling")
        for name in ("enemy_effect", "friendly_effect"):
            if self.pooling[name] not in EFFECT_POOLING:
                raise ValueError(f"invalid {name} pooling")
        if self.pooling["continuation"] not in CONTINUATION_POOLING:
            raise ValueError("invalid continuation pooling")
        if self.pooling["phase"] not in ("sum", "transformer") \
                or self.pooling["generation_pool"] not in ("sum", "transformer"):
            raise ValueError("invalid summary pooling")

        self.concepts = ConceptEmbeddings(self.layout, width)
        self.encoders = nn.ModuleDict({
            name: TokenEncoder(semantic, numeric, width)
            for name, _unsigned, _signed, semantic, numeric in TOKEN_SPECS
        })
        self.action_encoder = TokenEncoder(ACTION_FIELDS[2], ACTION_FIELDS[3], width)
        summary_names = (*POOLING_DEFAULTS, "card_pool", "relic_pool", "encounter_pool", "event_pool")
        self.summary_seed = nn.ParameterDict({name: nn.Parameter(torch.randn(width) * .02)
                                              for name in summary_names})

        def transformer(layer_count=1):
            layer = nn.TransformerEncoderLayer(
                width, heads, feedforward, dropout=0, batch_first=True,
                norm_first=True, activation="gelu",
            )
            return nn.TransformerEncoder(layer, layer_count, enable_nested_tensor=False)

        self.pool_transformers = nn.ModuleDict()
        for name, mode in self.pooling.items():
            if mode in ("transformer", "transformer_into_actor", "transformer_token"):
                key = "generation_pool" if name == "generation_pool" else name
                if key not in self.pool_transformers:
                    self.pool_transformers[key] = transformer()
        self.move_gru = nn.GRU(width, width, batch_first=True)
        self.continuation_gru = nn.GRU(width, width, batch_first=True) \
            if self.pooling["continuation"] == "gru" else None
        self.actor_norm = nn.LayerNorm(width)
        self.action_norm = nn.LayerNorm(width)
        self.global_transformer = transformer(layers)
        self.global_norm = nn.LayerNorm(width)

        self.graph_norm = nn.LayerNorm(width)
        self.graph_query = nn.Linear(width, width)
        self.graph_key_value = nn.Linear(width, 2 * width)
        self.graph_edge = nn.Linear(width, 2 * width, bias=False)
        self.graph_out = nn.Linear(width, width)
        self.graph_degree = nn.Linear(2, width, bias=False)
        self.graph_ff_norm = nn.LayerNorm(width)
        self.graph_ff = nn.Sequential(
            nn.Linear(width, feedforward), nn.GELU(), nn.Linear(feedforward, width),
        )
        self.policy = nn.Linear(width, 1)
        self.critic = nn.Linear(width, 1)
        self._graph_cache = {}
        self.cache_stats = {"graph_hit": 0, "graph_miss": 0}
        nn.init.normal_(self.policy.weight, std=.01)
        nn.init.zeros_(self.policy.bias)
        nn.init.zeros_(self.critic.weight)
        nn.init.zeros_(self.critic.bias)

    def load_state_dict(self, state_dict, strict=True, assign=False):
        self._graph_cache.clear()
        self.cache_stats = dict.fromkeys(self.cache_stats, 0)
        return nn.Module.load_state_dict(self, state_dict, strict, assign)

    def train(self, mode=True):
        if mode:
            self._graph_cache.clear()
            self.cache_stats = dict.fromkeys(self.cache_stats, 0)
        return nn.Module.train(self, mode)

    def attend(self, query, key_value, sequence):
        offsets, rows, position, max_length = sequence
        if query.device.type == "mps":
            return _RaggedSummaryAttention.apply(query, key_value, offsets, rows, self.heads)
        lengths = offsets[1:] - offsets[:-1]
        valid = torch.arange(max_length, device=query.device)[None] < lengths[:, None]
        padded = key_value.new_zeros((len(query), max_length, 2 * self.width))
        padded[rows, position] = key_value
        query = query.reshape(len(query), self.heads, 1, -1)
        key, value = padded.reshape(len(query), max_length, 2, self.heads, -1).unbind(2)
        return nn.functional.scaled_dot_product_attention(
            query, key.transpose(1, 2), value.transpose(1, 2),
            attn_mask=valid[:, None, None], dropout_p=0,
        ).transpose(1, 2).reshape(len(query), self.width)

    @staticmethod
    def group_log_softmax(score, legal, index):
        offsets, group, _position, _maximum = index
        if score.device.type == "mps":
            return _RaggedLogSoftmax.apply(score.masked_fill(~legal, -torch.inf), offsets)
        score = score.masked_fill(~legal, -torch.inf)
        normalizer = score.new_full((len(offsets) - 1,), -torch.inf).scatter_reduce_(
            0, group, score, reduce="amax",
        )
        normalizer = normalizer + score.new_zeros(len(normalizer)).index_add(
            0, group, (score - normalizer[group]).exp(),
        ).log()
        value = score - torch.where(torch.isfinite(normalizer), normalizer, 0)[group]
        return torch.where(legal, value, torch.zeros_like(value))

    def encode_map(self, encoded, index):
        node_source, edge_source, levels, offsets, current, current_inverse, current_source, \
            current_sequence, keys = index
        cached = [self._graph_cache.get(key) for key in keys]
        if not self.training and node_source.device.type == "cpu" and all(x is not None for x in cached):
            self.cache_stats["graph_hit"] += len(keys)
            nodes = torch.cat(cached)
        else:
            if not self.training and node_source.device.type == "cpu":
                self.cache_stats["graph_hit"] += sum(x is not None for x in cached)
                self.cache_stats["graph_miss"] += sum(x is None for x in cached)
            nodes = encoded[DOMAIN["map_node"]][node_source]
            if not self.training and node_source.device.type == "cpu":
                ranges = offsets.tolist()
                for graph, value in enumerate(cached):
                    if value is not None:
                        nodes[ranges[graph]:ranges[graph + 1]] = value
            edges = encoded[DOMAIN["map_edge"]][edge_source]
            for parents, children, edge_rows, sequence, degree, count, _edge_count in levels:
                attended = self.attend(
                    self.graph_query(self.graph_norm(nodes[parents])),
                    self.graph_key_value(self.graph_norm(nodes[children]))
                    + self.graph_edge(edges[edge_rows]), sequence,
                )
                features = torch.stack((degree / 8, degree.log1p() / 3), 1).to(nodes.dtype)
                updated = nodes[parents] + self.graph_out(attended) + self.graph_degree(features)
                updated = updated + self.graph_ff(self.graph_ff_norm(updated))
                nodes.index_copy_(0, parents[:count], updated[:count])
            if not self.training and node_source.device.type == "cpu":
                offsets = offsets.tolist()
                for graph, key in enumerate(keys):
                    if cached[graph] is None:
                        self._graph_cache[key] = nodes[offsets[graph]:offsets[graph + 1]].detach()
                while len(self._graph_cache) > 8192:
                    self._graph_cache.pop(next(iter(self._graph_cache)))
        selected = nodes[current]
        attended = self.attend(
            self.graph_query(self.graph_norm(selected)),
            self.graph_key_value(self.graph_norm(nodes[current_source])), current_sequence,
        )
        selected = selected + self.graph_out(attended)
        selected = selected + self.graph_ff(self.graph_ff_norm(selected))
        return selected[current_inverse], nodes

    def encode_domains(self, domains, concepts):
        return tuple(self.encoders[name](*values[:2], concepts)[:values[2]]
                     for (name, *_), values in zip(TOKEN_SPECS, domains))

    def _tag(self, values, role, collection=None):
        tagged = values + self.concepts.local("token_role", role).to(values.dtype)
        return tagged if collection is None else \
            tagged + self.concepts.local("collection", collection).to(values.dtype)

    @staticmethod
    def _domain_rows(domains, encoded, domain, predicate=None, scope_value=-1):
        _semantic, _numeric, _count, u, row, scope, inverse = domains[domain]
        selected = scope == scope_value
        if predicate is not None:
            selected &= predicate(u)
        source = selected.nonzero().squeeze(1)
        return encoded[domain][inverse[source]], row[source], u[source]

    @staticmethod
    def _ordered(values, group):
        if len(group) < 2:
            return values, group
        order = torch.argsort(group, stable=True)
        return values[order], group[order]

    def _sequence(self, values, group, groups, seed, transformer, selected=None):
        order = torch.argsort(group, stable=True)
        inverse = torch.empty_like(order)
        inverse[order] = torch.arange(len(order), device=order.device)
        values, group = values[order], group[order]
        selected_count = selected
        selected = (torch.arange(len(values), device=values.device) >= len(values) - selected)[order] \
            if selected is not None else None
        counts = torch.bincount(group, minlength=groups)
        if values.device.type == "mps" and self.width // self.heads in (16, 32):
            lengths = counts + 1
            padded_groups = (groups // 256 + 1) * 256
            lengths = nn.functional.pad(lengths, (0, padded_groups - groups), value=1)
            total = len(values) + padded_groups
            query_size = 0 if not selected_count else min(
                size for power in range(max(1, (selected_count - 1).bit_length()), 64)
                for size in (3 * (1 << power) // 4, 1 << power) if size >= selected_count
            )
            query_extra = query_size - (selected_count or 0)
            quantum = 65_536 if groups >= 4096 else 4096
            extra = (-total) % quantum
            if extra < query_extra:
                extra += quantum
            lengths[groups:] += extra // (padded_groups - groups)
            lengths[groups:groups + extra % (padded_groups - groups)] += 1
            total += extra
            offsets = torch.cat((counts.new_zeros(1), lengths.cumsum(0)))
            starts = torch.repeat_interleave(offsets[:groups], counts)
            position = torch.arange(len(values), device=values.device) \
                - torch.repeat_interleave(counts.cumsum(0) - counts, counts) + 1
            destination = starts + position
            current = values.new_zeros((total, self.width))
            current[offsets[:groups]] = seed.to(values.dtype)
            current[destination] = values
            sequence = torch.repeat_interleave(
                torch.arange(len(lengths), device=values.device), lengths,
            )
            for layer_index, layer in enumerate(transformer.layers):
                if selected is not None and layer_index == len(transformer.layers) - 1:
                    parts = [offsets[:-1], destination[selected]]
                    if query_extra:
                        dummy_counts = lengths[groups:] - 1
                        starts = torch.repeat_interleave(offsets[groups:-1] + 1, dummy_counts)
                        position = torch.arange(extra, device=values.device) \
                            - torch.repeat_interleave(dummy_counts.cumsum(0) - dummy_counts,
                                                     dummy_counts)
                        parts.append((starts + position)[:query_extra])
                    positions, query_order = torch.cat(parts).sort()
                    real = (query_order >= len(lengths)) & \
                        (query_order < len(lengths) + selected_count)
                    query_counts = torch.bincount(sequence[positions], minlength=len(lengths))
                    query_offsets = torch.cat((query_counts.new_zeros(1), query_counts.cumsum(0)))
                    qkv = nn.functional.linear(
                        layer.norm1(current), layer.self_attn.in_proj_weight,
                        layer.self_attn.in_proj_bias,
                    )
                    attended = _RaggedAttention.apply(
                        qkv, offsets.to(torch.int32), sequence.to(torch.int32),
                        layer.self_attn.num_heads, positions.to(torch.int32),
                        query_offsets.to(torch.int32), True,
                    )
                    current = current[positions] + layer.self_attn.out_proj(attended)
                    current = current + layer.linear2(layer.activation(layer.linear1(layer.norm2(current))))
                    state = current[query_offsets[:groups]]
                    items = current[real][torch.argsort(order[selected])]
                    return state, items
                qkv = nn.functional.linear(
                    layer.norm1(current), layer.self_attn.in_proj_weight,
                    layer.self_attn.in_proj_bias,
                )
                attended = _RaggedAttention.apply(
                    qkv, offsets.to(torch.int32), sequence.to(torch.int32),
                    layer.self_attn.num_heads, sequence.to(torch.int32), offsets.to(torch.int32),
                    False,
                )
                current = current + layer.self_attn.out_proj(attended)
                current = current + layer.linear2(layer.activation(layer.linear1(layer.norm2(current))))
            items = current[destination][inverse] if len(values) else values
            return current[offsets[:groups]], items
        maximum = int(counts.max().item()) if len(counts) else 0
        sequence = values.new_zeros((groups, maximum + 1, self.width))
        sequence[:, 0] = seed.to(values.dtype)
        if len(values):
            starts = torch.repeat_interleave(counts.cumsum(0) - counts, counts)
            position = torch.arange(len(values), device=values.device) - starts + 1
            sequence[group, position] = values
        valid = torch.arange(maximum + 1, device=values.device)[None] <= counts[:, None]
        sequence = transformer(sequence, src_key_padding_mask=~valid)
        items = sequence[group, position][inverse] if len(values) else values
        if selected is not None:
            items = items[selected[inverse]]
        return sequence[:, 0], items

    def _summarize(self, values, group, groups, name, mode, transformer_name=None):
        seed = self.summary_seed[name]
        if mode == "sum":
            summary = values.new_zeros((groups, self.width))
            if len(values):
                summary.index_add_(0, group, values.to(summary.dtype))
        elif mode == "gru":
            values, group = self._ordered(values, group)
            counts = torch.bincount(group, minlength=groups)
            maximum = int(counts.max().item()) if len(counts) else 0
            padded = values.new_zeros((groups, maximum, self.width))
            if len(values):
                starts = torch.repeat_interleave(counts.cumsum(0) - counts, counts)
                position = torch.arange(len(values), device=values.device) - starts
                padded[group, position] = values
                output, _ = self.continuation_gru(padded)
                present = (counts > 0).nonzero().squeeze(1)
                summary = values.new_zeros((groups, self.width)).index_copy(
                    0, present, output[present, counts[present] - 1],
                )
            else:
                summary = values.new_zeros((groups, self.width))
        else:
            summary, _ = self._sequence(values, group, groups, seed,
                                        self.pool_transformers[transformer_name or name])
        return summary

    def _collection(self, values, rows, active, name):
        collection = ("deck", "hand", "draw", "discard", "exhaust", "relic", "potion", "orb").index(name)
        mode = self.pooling[name]
        if mode == "global_tokens":
            return self._tag(values, 9, collection), rows
        summary = self._summarize(values, rows, int(active.max().item()) + 1 if len(active) else 0,
                                  name, mode)
        return self._tag(summary[active], 14, collection), active

    def _actors(self, domains, encoded, batch, concepts):
        actors, rows, u = self._domain_rows(domains, encoded, DOMAIN["actor"])
        if not len(actors):
            return actors, rows, u, [], []
        order = torch.argsort(rows * (1 << 20) + u[:, 0], stable=True)
        actors, rows, u = actors[order], rows[order], u[order]
        keys = rows * (1 << 20) + u[:, 0]

        history, history_rows, history_u = self._domain_rows(domains, encoded, DOMAIN["history"])
        if len(history):
            history_keys = history_rows * (1 << 20) + history_u[:, 0]
            group = torch.searchsorted(keys, history_keys)
            valid = group < len(keys)
            valid &= keys[group.clamp_max(len(keys) - 1)] == history_keys
            player = valid & (history_u[:, 1] == 0)
            if player.any():
                actors = actors.index_add(0, group[player], history[player].to(actors.dtype))
            moves = valid & (history_u[:, 1] == 1)
            if moves.any():
                history_semantic, _numeric, _count, _u, _row, _scope, history_inverse = \
                    domains[DOMAIN["history"]]
                raw = (domains[DOMAIN["history"]][5] == -1).nonzero().squeeze(1)[moves]
                move_values = nn.functional.embedding(
                    history_semantic[history_inverse[raw], :1].long(), concepts,
                ).squeeze(1)
                move_values, move_group = self._ordered(move_values, group[moves])
                counts = torch.bincount(move_group, minlength=len(actors))
                maximum = int(counts.max().item())
                padded = move_values.new_zeros((len(actors), maximum, self.width))
                starts = torch.repeat_interleave(counts.cumsum(0) - counts, counts)
                position = torch.arange(len(move_values), device=move_values.device) - starts
                padded[move_group, position] = move_values
                output, _ = self.move_gru(padded)
                present = (counts > 0).nonzero().squeeze(1)
                move = move_values.new_zeros((len(actors), self.width)).index_copy(
                    0, present, output[present, counts[present] - 1],
                )
                actors = actors + move

        effect_values, effect_groups = [], []
        for domain in (DOMAIN["power"], DOMAIN["status"]):
            values, effect_rows, effect_u = self._domain_rows(domains, encoded, domain)
            if not len(values):
                continue
            effect_keys = effect_rows * (1 << 20) + effect_u[:, 0]
            group = torch.searchsorted(keys, effect_keys)
            valid = group < len(keys)
            valid &= keys[group.clamp_max(len(keys) - 1)] == effect_keys
            effect_values.append(values[valid])
            effect_groups.append(group[valid])
        effects = torch.cat(effect_values) if effect_values else actors.new_empty((0, self.width))
        effect_group = torch.cat(effect_groups) if effect_groups else rows.new_empty(0)
        extra_values, extra_rows = [], []
        for name, family in (("friendly_effect", u[:, 1] < 2), ("enemy_effect", u[:, 1] == 2)):
            actor_index = family.nonzero().squeeze(1)
            if not len(actor_index):
                continue
            lookup = rows.new_full((len(actors),), -1)
            lookup[actor_index] = torch.arange(len(actor_index), device=rows.device)
            selected = lookup[effect_group] >= 0 if len(effect_group) else effect_group.bool()
            values = effects[selected]
            local_group = lookup[effect_group[selected]]
            mode = self.pooling[name]
            if mode == "global_tokens":
                extra_values.append(self._tag(values, 10, 8 + (name == "enemy_effect")))
                extra_rows.append(rows[effect_group[selected]])
                continue
            pooled_mode = "transformer" if mode.startswith("transformer") else "sum"
            summary = self._summarize(values, local_group, len(actor_index), name, pooled_mode)
            if mode.endswith("into_actor"):
                actors = actors.index_add(0, actor_index, summary.to(actors.dtype))
            else:
                extra_values.append(self._tag(summary, 10, 8 + (name == "enemy_effect")))
                extra_rows.append(rows[actor_index])
        actors = self.actor_norm(self._tag(actors, 8))
        return actors, rows, u, extra_values, extra_rows

    def _actions(self, domains, encoded, values, index, nodes, concepts):
        semantic, numeric = values
        action_row, action_flat, _legal, policy_sequence, actions, path, action_count = index
        action = self.action_encoder(semantic, numeric, concepts)[:action_count]
        attached = action.new_zeros(action.shape)
        for domain in range(len(TOKEN_SPECS)):
            _semantic, _numeric, _count, _u, _row, scope, inverse = domains[domain]
            selected = (scope >= 0).nonzero().squeeze(1)
            if len(selected):
                attached.index_add_(0, scope[selected], encoded[domain][inverse[selected]].to(attached.dtype))
        present = path[:action_count] >= 0
        if present.any():
            attached[present] += nodes[path[:action_count][present]]
        action = self.action_norm(self._tag(action + attached, 13))
        return action, action_row[:action_count], action_flat[:action_count], actions, policy_sequence

    def _continuations(self, domains, encoded):
        values, rows, u = self._domain_rows(domains, encoded, DOMAIN["continuation"])
        if not len(values):
            return values, rows
        order = torch.argsort(rows * (1 << 32) + u[:, 8], stable=True)
        values, rows, u = values[order], rows[order], u[order]
        keys = rows * (1 << 32) + u[:, 2]
        first = torch.ones(len(keys), dtype=torch.bool, device=keys.device)
        first[1:] = keys[1:] != keys[:-1]
        group = first.cumsum(0) - 1
        items = values.new_zeros((int(group[-1]) + 1, self.width))
        items.index_add_(0, group, values)
        return items, rows[first]

    def encode_state(self, domains, encoded, index, action_values, action_index, concepts):
        batch = int(domains[DOMAIN["run"]][4].max().item()) + 1
        current, nodes = self.encode_map(encoded, index)
        action, action_rows, action_flat, actions, policy_sequence = self._actions(
            domains, encoded, action_values, action_index, nodes, concepts,
        )
        values, rows = [], []
        add = lambda value, row: (values.append(value), rows.append(row))

        run, run_rows, _ = self._domain_rows(domains, encoded, DOMAIN["run"])
        add(self._tag(run, 1), run_rows)
        phase, phase_rows, _ = self._domain_rows(domains, encoded, DOMAIN["phase"])
        phase_values, phase_groups = [phase], [phase_rows]
        for domain in range(len(TOKEN_SPECS)):
            item, item_rows, _ = self._domain_rows(
                domains, encoded, domain, scope_value=-2,
            )
            if len(item):
                phase_values.append(item); phase_groups.append(item_rows)
        phase = self._summarize(torch.cat(phase_values), torch.cat(phase_groups), batch,
                                "phase", self.pooling["phase"])
        add(self._tag(phase, 2), torch.arange(batch, device=phase.device))
        add(self._tag(current, 3), torch.arange(batch, device=current.device))

        pool_specs = (
            ("card_pool", DOMAIN["card"], lambda u: u[:, 0] == 13),
            ("relic_pool", DOMAIN["relic"], lambda u: (u[:, 0] == 1) | (u[:, 0] == 2)),
            ("encounter_pool", DOMAIN["encounter"], lambda u: u[:, 0] <= 2),
            ("event_pool", DOMAIN["event"], lambda u: u[:, 0] == 0),
        )
        for name, domain, predicate in pool_specs:
            item, item_rows, _ = self._domain_rows(domains, encoded, domain, predicate)
            summary = self._summarize(
                item, item_rows, batch, name, self.pooling["generation_pool"],
                transformer_name="generation_pool",
            )
            role = ("card_pool", "relic_pool", "encounter_pool", "event_pool").index(name) + 4
            add(self._tag(summary, role), torch.arange(batch, device=summary.device))

        actors, actor_rows, actor_u, effect_values, effect_rows = self._actors(
            domains, encoded, batch, concepts,
        )
        combat_rows = actor_rows[actor_u[:, 1] == 0]
        for name, domain, predicate, active in (
            ("deck", DOMAIN["card"], lambda u: u[:, 0] == 0, torch.arange(batch, device=run.device)),
            ("hand", DOMAIN["card"], lambda u: u[:, 0] == 1, combat_rows),
            ("draw", DOMAIN["card"], lambda u: u[:, 0] == 2, combat_rows),
            ("discard", DOMAIN["card"], lambda u: u[:, 0] == 3, combat_rows),
            ("exhaust", DOMAIN["card"], lambda u: u[:, 0] == 4, combat_rows),
            ("relic", DOMAIN["relic"], lambda u: u[:, 0] == 0, torch.arange(batch, device=run.device)),
            ("potion", DOMAIN["potion"], lambda u: u[:, 0] == 0, torch.arange(batch, device=run.device)),
            ("orb", DOMAIN["orb"], None, combat_rows),
        ):
            item, item_rows, item_u = self._domain_rows(domains, encoded, domain, predicate)
            if name == "relic" and len(item):
                stored, stored_rows, _ = self._domain_rows(
                    domains, encoded, DOMAIN["card"], lambda u: u[:, 0] == 8,
                )
                payload = item.new_zeros((batch, self.width))
                if len(stored):
                    payload.index_add_(0, stored_rows, stored.to(payload.dtype))
                pael = item_u[:, 9] != 0
                item = item + pael[:, None] * payload[item_rows]
            item, item_rows = self._collection(item, item_rows, active, name)
            add(item, item_rows)
        add(actors, actor_rows)
        values.extend(effect_values); rows.extend(effect_rows)

        continuation, continuation_rows = self._continuations(domains, encoded)
        mode = self.pooling["continuation"]
        if mode == "global_tokens":
            add(self._tag(continuation, 11, 10), continuation_rows)
        else:
            add(self._tag(self._summarize(
                    continuation, continuation_rows, batch, "continuation", mode), 14, 10),
                torch.arange(batch, device=run.device))
        crystal, crystal_rows, _ = self._domain_rows(domains, encoded, DOMAIN["crystal"])
        add(self._tag(crystal, 12), crystal_rows)
        add(action, action_rows)

        values = torch.cat(values)
        rows = torch.cat(rows)
        self._sequence_lengths = torch.bincount(rows, minlength=batch) + 1
        state, transformed = self._sequence(
            values, rows, batch, self.concepts.local("token_role", 0), self.global_transformer,
            len(action),
        )
        transformed = self.global_norm(transformed)
        action = transformed if len(action) else action
        return self.global_norm(state), action, action_rows, action_flat, actions, policy_sequence

    def forward(self, _character, _globals, domains, state_index, action_values, action_index,
                return_state=False, policy_only=False, flat_policy=False, temperature=1):
        concepts = self.concepts.flattened()
        encoded = self.encode_domains(domains, concepts)
        state, action, action_row, action_flat, actions, sequence = self.encode_state(
            domains, encoded, state_index, action_values, action_index, concepts,
        )
        scores = self.policy(action).squeeze(-1) / temperature
        legal = torch.ones_like(scores, dtype=torch.bool)
        scores = self.group_log_softmax(scores, legal, sequence)
        if flat_policy:
            policy = scores
        else:
            policy = scores.new_zeros(len(state) * actions).scatter(
                0, action_flat, scores,
            ).reshape(len(state), actions)
        if policy_only:
            return policy
        output = policy, self.critic(state)
        return (*output, state) if return_state else output


def architecture(model):
    return {
        "width": model.width, "layers": model.layers, "heads": model.heads,
        "feedforward": model.feedforward, "head_width": model.head_width,
        "domains": [name for name, *_ in TOKEN_SPECS], "card_zones": model.card_zones,
        "globals": model.layout["globals"], "entity_collections": model.entity_collections,
        "state_width": model.state_width, "policy_input": model.width,
        "map_layers": 1, "global_layers": model.layers, "local_layers": 1,
        "attention": "fully_bidirectional",
        "activation": "gelu", "dropout": 0,
        "token_encoder": "LayerNorm(sum(categorical)+Linear(numeric,bias=False))",
        "sum_pooling": "unnormalized",
        "policy_factorization": "one linear logit per transformed legal action token",
        "policy_entropy": ["candidate", "normalized", "effective_actions"],
        "map": "reverse-topological sparse attention; current/entry query over all contextual nodes",
        "move_history": f"one-layer {model.width}-D GRU",
        "continuations": "normalized execution queue",
        "candidate_context": "explicit selected payload, destination, and enemy identity+position",
        "card_zone_names": ["deck", "hand", "draw", "discard", "exhaust"],
        "fixed_tokens": ["state", "run", "phase", "current_map_node", "card_pool",
                         "relic_pool", "encounter_pool", "event_pool"],
        "concept_vocab": model.concepts.num_embeddings,
        "semantic_tables": dict(zip(SEMANTIC_NAMES, model.concepts.sizes)),
        "position_caps": POSITION_CAPS,
        "pooling": model.pooling,
        "candidate_numeric": ACTION_FIELDS[3],
        "value_heads": ["shaped_terminal_floor_scalar"],
        "potential_weights": model.potential_weights,
        "winning_reservoir": WINNING_CAPACITY,
    }


def predict(model, inputs, precision, temperature=1, policy_only=False, flat_policy=False):
    if precision not in PRECISIONS:
        raise ValueError("precision must be fp32 or bf16")
    kind = inputs[0].device.type
    if precision != "fp32" and kind not in ("mps", "cuda"):
        raise ValueError(f"{precision} requires MPS or CUDA")
    dtype = torch.bfloat16 if precision == "bf16" else torch.float16
    with torch.autocast(kind, dtype=dtype, enabled=precision != "fp32"):
        output = model(
            *inputs[:6], temperature=temperature, policy_only=policy_only,
            flat_policy=flat_policy,
        )
    return output.float() if policy_only else tuple(value.float() for value in output)


def predict_cached(model, inputs, precision, temperature=1):
    state, action, offsets, action_rows = inputs
    kind = state.device.type
    dtype = torch.bfloat16 if precision == "bf16" else torch.float16
    with torch.autocast(kind, dtype=dtype, enabled=precision != "fp32"):
        scores = model.policy(action).squeeze(-1) / temperature
        policy = model.group_log_softmax(
            scores, torch.ones_like(scores, dtype=torch.bool),
            (offsets, action_rows, None, None),
        )
        critic = model.critic(state)
    return policy.float(), critic.float()


def cached_tensors(features, width):
    rows = [np.frombuffer(row, "<f4").reshape(-1, width) for row in features]
    lengths = np.asarray([len(row) - 1 for row in rows], np.int32)
    offsets = np.r_[0, np.cumsum(lengths, dtype=np.int64)].astype(np.int32)
    return (
        torch.from_numpy(np.stack([row[0] for row in rows])),
        torch.from_numpy(np.concatenate([row[1:] for row in rows])),
        torch.from_numpy(offsets),
        torch.from_numpy(np.repeat(np.arange(len(rows), dtype=np.int64), lengths)),
    )


def critic_value(output):
    return output.squeeze(-1)


def critic_win_probability(value):
    floor = MAX_PROGRESS / (CATEGORIES - 1)
    return ((value - floor) / (1 - floor)).clamp(0, 1)


def critic_explained_reward_variance(predictions, targets, floors=None):
    prediction = predictions.float().detach().cpu().numpy()
    target = targets.float().detach().cpu().numpy()
    def explained(mask=slice(None)):
        variance = target[mask].var()
        value = float(1 - (target[mask] - prediction[mask]).var() / variance) \
            if variance else 0.
        return value, float(variance)
    overall = explained()[0]
    if floors is None:
        return overall
    floors = np.asarray(floors)
    grouped = {}
    for floor in np.unique(floors):
        mask = floors == floor
        value, variance = explained(mask)
        grouped[str(int(floor))] = {
            "value": value, "target_variance": variance, "rows": int(mask.sum()),
        }
    weight = sum(row["rows"] * row["target_variance"] for row in grouped.values())
    conditioned = sum(row["value"] * row["rows"] * row["target_variance"]
                      for row in grouped.values()) / weight if weight else 0.
    return overall, conditioned, grouped

def candidate_index(row, batch, target, allow_empty=False):
    row = np.asarray(row, np.int32)
    lengths = np.bincount(row, minlength=batch).astype(np.int32)
    if not allow_empty and not np.all(lengths):
        raise ValueError("state without represented candidate")
    offsets = np.r_[0, np.cumsum(lengths, dtype=np.int32)].astype(np.int32)
    starts = np.repeat(offsets[:-1], lengths)
    order = np.argsort(row, kind="stable")
    position = np.empty(len(row), np.int32)
    position[order] = np.arange(len(row), dtype=np.int32) - starts
    return tuple(torch.as_tensor(value, device=target) for value in (
        offsets, row, position,
    )) + (int(lengths.max(initial=0)),)


def observation_legal(observation):
    return np.asarray(observation[3][-1], bool)


def flat_domains(observation):
    batch = len(observation[0]); legal = observation_legal(observation)
    flat = len(observation[3]) == 7
    represented = None if flat else np.asarray(observation[3][4], bool)
    if flat:
        action_row, action_position = map(lambda value: np.asarray(value, np.int32), observation[3][4:6])
    else:
        action_row, action_position = np.nonzero(represented)
        lookup = np.full(represented.shape, -1, np.int32)
        lookup[action_row, action_position] = np.arange(len(action_row), dtype=np.int32)
    domains = []
    for (name, unsigned, signed, semantic, numeric), values in zip(TOKEN_SPECS, observation[2]):
        values = tuple(map(np.asarray, values))
        if flat:
            u, s, c, f, row, scope = values
        else:
            u, s, c, f, scope, mask = values
            if any(value.shape[:2] != mask.shape for value in (u, s, c, f)) or scope.shape != mask.shape:
                raise ValueError(f"invalid {name} shape")
            selected = mask.astype(bool)
            row = np.repeat(np.arange(batch, dtype=np.int32), selected.sum(1))
            u, s, c, f, scope = (value[selected] for value in (u, s, c, f, scope))
            scope = scope.astype(np.int32)
            candidate = scope >= 0
            if candidate.any():
                scope[candidate] = lookup[row[candidate], scope[candidate]]
                if np.any(scope[candidate] < 0):
                    raise ValueError("domain row references absent candidate")
        if (u.dtype != np.uint32 or s.dtype != np.int32 or c.dtype != np.uint32
                or f.dtype != np.float32 or u.shape != (len(row), unsigned)
                or s.shape != (len(row), signed) or c.shape != (len(row), semantic)
                or f.shape != (len(row), numeric) or scope.shape != (len(row),)
                or not np.isfinite(f).all()):
            raise ValueError(f"invalid {name} schema")
        domains.append((u, s, c, f, np.asarray(row, np.int32), np.asarray(scope, np.int32)))
    return domains, np.asarray(action_row, np.int32), np.asarray(action_position, np.int32), legal


def ordered(source, *keys):
    if len(source) < 2:
        return source
    values = [np.asarray(key)[source] for key in keys]
    equal = np.ones(len(source) - 1, bool)
    for value in values:
        if np.any(equal & (value[1:] < value[:-1])):
            return source[np.lexsort(tuple(reversed(values)))]
        equal &= value[1:] == value[:-1]
    return source


def model_inputs(character, globals_, domains, action, action_row, action_position, legal, target, model):
    batch, actions = len(character), legal.shape[1]
    globals_ = np.asarray(globals_, np.float32)
    if globals_.shape != (batch, model.layout["globals"]) or not np.isfinite(globals_).all():
        raise ValueError("invalid public globals")
    pad = model.training and model.global_norm.weight.device.type == "mps"
    bucket = lambda count: 0 if not count else min(
        size for power in range(max(1, (count - 1).bit_length()), 64)
        for size in (3 * (1 << power) // 4, 1 << power) if size >= count
    )
    tensor = lambda value: torch.as_tensor(value, dtype=torch.long, device=target)
    domain_tensors = []
    inverse_rows = []
    for (name, *_), (u, _s, semantic, numeric, row, scope) in zip(TOKEN_SPECS, domains):
        if semantic.max(initial=0) >= model.concepts.num_embeddings:
            raise ValueError(f"invalid {name} semantic id")
        if len(semantic):
            first, inverse = map(np.asarray, sts2_sim.unique_feature_rows(
                semantic, numeric.view(np.uint32),
            ))
        else:
            first = inverse = np.empty(0, np.intp)
        count = len(first)
        size = bucket(count) if pad else count
        domain_tensors.append((
            torch.as_tensor(np.pad(
                semantic[first], ((0, size - count), (0, 0)),
            ).astype(np.int32), device=target),
            torch.as_tensor(np.pad(
                numeric[first], ((0, size - count), (0, 0)),
            ), device=target), count,
            torch.as_tensor(u.astype(np.int64), device=target),
            torch.as_tensor(row.astype(np.int64), device=target),
            torch.as_tensor(scope.astype(np.int64), device=target),
            torch.as_tensor(inverse.astype(np.int64), device=target),
        ))
        inverse_rows.append(inverse)
    domain_tensors = tuple(domain_tensors)

    node_u, _node_s, node_c, node_f, node_row, node_scope = domains[DOMAIN["map_node"]]
    edge_u, _edge_s, edge_c, edge_f, edge_row, edge_scope = domains[DOMAIN["map_edge"]]
    node_source = ordered(np.flatnonzero(node_scope == -1).astype(np.int32), node_row, node_u[:, 0])
    edge_source = ordered(np.flatnonzero(edge_scope == -1).astype(np.int32),
                          edge_row, edge_u[:, 0], edge_u[:, 1])
    node_offsets = np.r_[0, np.cumsum(np.bincount(node_row[node_source], minlength=batch))]
    edge_offsets = np.r_[0, np.cumsum(np.bincount(edge_row[edge_source], minlength=batch))]
    node_groups = [node_source[node_offsets[row]:node_offsets[row + 1]] for row in range(batch)]
    edge_groups = [edge_source[edge_offsets[row]:edge_offsets[row + 1]] for row in range(batch)]
    if model.training:
        representatives, inverse = map(np.asarray, sts2_sim.unique_graphs(
            node_u, node_c, node_f, node_source, node_offsets,
            edge_u, edge_c, edge_f, edge_source, edge_offsets,
        ))
        representatives = representatives.astype(np.int32)
        inverse = inverse.astype(np.int32)
        graph_keys = ()
    else:
        graph_keys = tuple((
            node_u[nodes].tobytes(), node_c[nodes].tobytes(), node_f[nodes].tobytes(),
            edge_u[edges].tobytes(), edge_c[edges].tobytes(), edge_f[edges].tobytes(),
        ) for nodes, edges in zip(node_groups, edge_groups))
        unique, representatives, inverse = {}, [], np.empty(batch, np.int32)
        for row, key in enumerate(graph_keys):
            if key not in unique:
                unique[key] = len(representatives); representatives.append(row)
            inverse[row] = unique[key]
    node_source = np.concatenate([node_groups[row] for row in representatives])
    edge_source = np.concatenate([edge_groups[row] for row in representatives])
    node_offsets = np.r_[0, np.cumsum([len(node_groups[row]) for row in representatives])]
    graph_for_node = np.repeat(np.arange(len(representatives)), np.diff(node_offsets))
    missing = np.asarray([
        model.training or target.type != "cpu" or graph_keys[row] not in model._graph_cache
        for row in representatives
    ])
    max_node = int(node_u[node_source, 0].max(initial=0))
    node_lookup = np.full((batch, max_node + 1), -1, np.int32)
    offset = 0
    for graph, row in enumerate(representatives):
        ids = node_u[node_groups[row], 0].astype(np.intp)
        members = np.flatnonzero(inverse == graph)
        node_lookup[members[:, None], ids] = offset + np.arange(len(ids))
        offset += len(ids)
    edge_ids = edge_u[edge_source, :2].astype(np.intp)
    if edge_ids.max(initial=0) > max_node:
        raise ValueError("map edge references missing node")
    edge_src = node_lookup[edge_row[edge_source], edge_ids[:, 0]]
    edge_dst = node_lookup[edge_row[edge_source], edge_ids[:, 1]]
    if np.any(edge_src < 0) or np.any(edge_dst < 0):
        raise ValueError("map edge references missing node")
    topo = node_u[node_source, 8].astype(np.int32)
    if len(edge_src) and np.any(topo[edge_dst] <= topo[edge_src]):
        raise ValueError("map is not a forward DAG")
    levels = []
    for level in sorted(np.unique(topo[edge_src]), reverse=True):
        selected = np.flatnonzero(topo[edge_src] == level)
        selected = selected[np.argsort(edge_src[selected], kind="stable")]
        parents, group = np.unique(edge_src[selected], return_inverse=True)
        selected = selected[missing[graph_for_node[parents]][group]]
        if len(selected):
            parents, group = np.unique(edge_src[selected], return_inverse=True)
            levels.append((
                parents, edge_dst[selected], selected, group,
                node_u[node_source[parents], 9].astype(np.float32),
            ))
    if pad:
        padded = []
        for parents, children, edges, group, degree in levels:
            count, edge_count = len(parents), len(children)
            edge_size = bucket(edge_count)
            parent_size = bucket(count + (edge_size > edge_count))
            extra = edge_size - edge_count
            padded.append((
                tensor(np.pad(parents, (0, parent_size - count))),
                tensor(np.pad(children, (0, extra))), tensor(np.pad(edges, (0, extra))),
                candidate_index(np.r_[group, np.full(extra, count, np.int32)],
                                parent_size, target, True),
                torch.as_tensor(np.pad(degree, (0, parent_size - count)), device=target),
                count, edge_count,
            ))
        levels = padded
    else:
        levels = [(
            tensor(parents), tensor(children), tensor(edges),
            candidate_index(group, len(parents), target),
            torch.as_tensor(degree, device=target), len(parents), len(children),
        ) for parents, children, edges, group, degree in levels]
    run_u, _run_s, _run_c, _run_f, run_row, run_scope = domains[DOMAIN["run"]]
    run_source = np.flatnonzero(run_scope == -1)
    current_id = np.full(batch, np.iinfo(np.uint32).max, np.uint32)
    current_id[run_row[run_source]] = run_u[run_source, 23]
    current = np.full(batch, -1, np.int32)
    present = current_id != np.iinfo(np.uint32).max
    if current_id[present].max(initial=0) > max_node:
        raise ValueError("current map node is missing")
    current[present] = node_lookup[np.flatnonzero(present), current_id[present].astype(np.intp)]
    entry = node_lookup[:, 0]
    if np.any(current[present] < 0) or np.any(entry < 0):
        raise ValueError("current map node is missing")
    current, current_inverse = np.unique(np.where(current >= 0, current, entry), return_inverse=True)
    current_graph = np.searchsorted(node_offsets[1:], current, side="right")
    current_count = np.diff(node_offsets)[current_graph]
    current_source = np.concatenate([
        np.arange(node_offsets[graph], node_offsets[graph + 1], dtype=np.int32)
        for graph in current_graph
    ])
    current_group = np.repeat(np.arange(len(current), dtype=np.int32), current_count)
    map_index = (
        tensor(inverse_rows[DOMAIN["map_node"]][node_source]),
        tensor(inverse_rows[DOMAIN["map_edge"]][edge_source]), tuple(levels),
        tensor(node_offsets), tensor(current), tensor(current_inverse), tensor(current_source),
        candidate_index(current_group, len(current), target),
        tuple(graph_keys[row] for row in representatives) if graph_keys else (),
    )

    action_u, _action_s, action_c, action_f = action
    action_count = len(action_u)
    action_size = bucket(action_count) if pad else action_count
    if action_c.max(initial=0) >= model.concepts.num_embeddings:
        raise ValueError("invalid action semantic id")
    path = np.full(action_count, -1, np.int32)
    path_id = action_u[:, 4]
    present = path_id != np.iinfo(np.uint32).max
    if path_id[present].max(initial=0) > max_node:
        raise ValueError("path candidate references missing node")
    path[present] = node_lookup[action_row[present], path_id[present].astype(np.intp)]
    if np.any(path[present] < 0):
        raise ValueError("path candidate references missing node")
    action_flat = action_row * actions + action_position
    action_legal = legal[action_row, action_position]
    action_index = (
        tensor(action_row), tensor(action_flat), torch.as_tensor(action_legal, device=target),
        candidate_index(action_row, batch, target, True), actions, tensor(path), action_count,
    )
    return (
        torch.as_tensor(character, dtype=torch.long, device=target),
        torch.as_tensor(globals_, device=target), domain_tensors, map_index,
        (torch.as_tensor(np.pad(
            action_c, ((0, action_size - action_count), (0, 0)),
        ).astype(np.int32), device=target),
         torch.as_tensor(np.pad(
             action_f, ((0, action_size - action_count), (0, 0)),
         ).astype(np.float32), device=target)),
        action_index, torch.as_tensor(legal, device=target),
    )


def _tensors(observation, target, model):
    domains, action_row, action_position, legal = flat_domains(observation)
    if len(observation[3]) == 7:
        action_u, action_s, action_c, action_f = map(np.asarray, observation[3][:4])
        represented = np.ones(len(action_u), bool)
    else:
        action_u, action_s, action_c, action_f, represented, _legal = map(np.asarray, observation[3])
        represented = represented.astype(bool)
    if (action_u.dtype != np.uint32 or action_s.dtype != np.int32 or action_c.dtype != np.uint32
            or action_f.dtype != np.float32
            or action_u.shape[-1] != ACTION_FIELDS[0] or action_s.shape[-1] != ACTION_FIELDS[1]
            or action_c.shape[-1] != ACTION_FIELDS[2] or action_f.shape[-1] != ACTION_FIELDS[3]
            or not np.isfinite(action_f[represented]).all()):
        raise ValueError("invalid action schema")
    return model_inputs(
        np.asarray(observation[0], np.uint8), np.asarray(observation[1], np.float32), domains,
        (action_u[represented], action_s[represented], action_c[represented], action_f[represented]),
        action_row, action_position, legal, target, model,
    )


def validate_packed(row):
    if isinstance(row, bytes):
        sts2_sim.validate_compact_observation(row)
        return
    if len(row) != 6:
        raise ValueError("packed observation lacks its Rust digest")
    character, globals_, counts, exact, actions, digest = row
    globals_, counts, actions = map(np.asarray, (globals_, counts, actions))
    compressed = isinstance(exact, bytes)
    if not compressed:
        exact = np.asarray(exact)
    if globals_.dtype != np.float32 or globals_.shape != (0,) or not np.isfinite(globals_).all() \
            or counts.dtype != np.uint32 or counts.ndim != 1 \
            or not compressed and (exact.dtype != np.uint32 or exact.ndim != 1) \
            or actions.dtype != np.uint32 or actions.ndim != 2:
        raise ValueError("invalid packed observation arrays")
    character, digest = int(character), int(digest)
    if character == np.iinfo(np.uint8).max:
        if (digest or globals_.any() or counts.shape != (len(TOKEN_SPECS),) or counts.any() or len(exact)
                or actions.shape != (0, sum(ACTION_FIELDS) + 1)):
            raise ValueError("invalid inactive observation")
    elif not 0 <= character < 5:
        raise ValueError("invalid observation character")
    else:
        sts2_sim.validate_packed_observation(character, globals_, counts, exact, actions, digest)


def packed_character(row):
    return row[4] if isinstance(row, bytes) else int(row[0])


def packed_action_count(row):
    return struct.unpack_from("<I", row, 16)[0] if isinstance(row, bytes) else len(row[4])


def packed_legal_count(row):
    return struct.unpack_from("<I", row, 20)[0] if isinstance(row, bytes) \
        else int(np.count_nonzero(row[4][:, -1]))


def packed_state_count(row):
    return sum(struct.unpack_from(f"<{len(TOKEN_SPECS)}I", row, 32)) \
        if isinstance(row, bytes) else int(row[2].sum())


def tensors(observation, target, model):
    return _tensors(observation, target, model)


def _pack_batch(observation):
    if len(observation) < 6:
        raise ValueError("observation lacks its Rust digest")
    if len(observation) > 6:
        return observation[6]
    character = np.asarray(observation[0], np.uint8)
    globals_ = np.asarray(observation[1], np.float32)
    digests = np.asarray(observation[5])
    represented = np.asarray(observation[3][4], bool)
    legal = np.asarray(observation[3][5], bool)
    batch = len(character)
    if globals_.shape != (batch, 0) or not np.isfinite(globals_).all() \
            or digests.dtype != np.uint64 or digests.shape != (batch,):
        raise ValueError("invalid observation digests")
    action_row, action_position = np.nonzero(represented)
    action_counts = np.bincount(action_row, minlength=batch)
    action_offsets = np.cumsum(action_counts) - action_counts
    remap = np.full(represented.shape, -1, np.int32)
    remap[action_row, action_position] = np.arange(len(action_row)) - np.repeat(
        action_offsets, action_counts
    )
    counts = np.empty((batch, len(TOKEN_SPECS)), np.uint32)
    pieces = [[] for _ in range(batch)]
    for domain, ((name, unsigned, signed, semantic, numeric), values) in enumerate(
        zip(TOKEN_SPECS, observation[2])
    ):
        u, s, c, f, scope, mask = map(np.asarray, values)
        row, _position = np.nonzero(mask.astype(bool))
        selected = mask.astype(bool)
        u, s, c, f, scope = (value[selected] for value in (u, s, c, f, scope))
        scope = scope.astype(np.int32)
        candidate = scope >= 0
        keep = ~candidate
        keep[candidate] = represented[row[candidate], scope[candidate]]
        u, s, c, f, scope, row = u[keep], s[keep], c[keep], f[keep], scope[keep], row[keep]
        candidate = scope >= 0
        scope[candidate] = remap[row[candidate], scope[candidate]]
        if (u.shape[1:] != (unsigned,) or s.shape[1:] != (signed,)
                or c.shape[1:] != (semantic,) or f.shape[1:] != (numeric,)):
            raise ValueError(f"invalid {name} schema")
        packed = np.concatenate((
            u.astype(np.uint32), s.astype(np.int32).view(np.uint32),
            c.astype(np.uint32), f.astype(np.float32).view(np.uint32),
            scope.view(np.uint32)[:, None],
        ), 1)
        sizes = np.bincount(row, minlength=batch)
        counts[:, domain] = sizes
        offsets = np.cumsum(sizes) - sizes
        for index, (offset, size) in enumerate(zip(offsets, sizes)):
            pieces[index].append(packed[offset:offset + size].reshape(-1))
    action_u, action_s, action_c, action_f = map(np.asarray, observation[3][:4])
    rows = []
    for index in range(batch):
        selected = represented[index]
        actions = np.concatenate((
            action_u[index, selected].astype(np.uint32),
            action_s[index, selected].astype(np.int32).view(np.uint32),
            action_c[index, selected].astype(np.uint32),
            action_f[index, selected].astype(np.float32).view(np.uint32),
            legal[index, selected].astype(np.uint32)[:, None],
        ), 1)
        rows.append((character[index], globals_[index], counts[index], np.concatenate(pieces[index]), actions,
                     digests[index]))
    return rows


def pack_batch(observation, validate=True):
    rows = _pack_batch(observation)
    if len(rows) != len(observation[0]):
        raise ValueError("packed observation row count mismatch")
    if validate:
        for row in rows:
            validate_packed(row)
    return rows



def unpack(rows, target, model, validate=True):
    if validate:
        for row in rows:
            validate_packed(row)
    character, globals_, domains, action, action_row, action_position, legal = \
        sts2_sim.unpack_packed_observations(rows)
    return model_inputs(
        np.asarray(character), np.asarray(globals_),
        [tuple(map(np.asarray, domain)) for domain in domains],
        tuple(map(np.asarray, action)), np.asarray(action_row), np.asarray(action_position),
        np.asarray(legal), target, model,
    )


def link_checkpoint(source, target):
    temporary = target.with_suffix(".tmp")
    temporary.unlink(missing_ok=True)
    os.link(source, temporary)
    temporary.replace(target)


def sha256_file(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def optimizer_compatible(state, parameter_count):
    return len([parameter for group in state["param_groups"] for parameter in group["params"]]) \
        == parameter_count


def optimizer_configuration(state):
    values = {
        (tuple(group.get("betas", ())), float(group.get("eps", math.nan)))
        for group in state["param_groups"]
    }
    if len(values) != 1:
        raise ValueError("checkpoint optimizer groups disagree")
    betas, eps = values.pop()
    if len(betas) != 2 or not all(map(math.isfinite, (*betas, eps))):
        raise ValueError("invalid checkpoint optimizer configuration")
    return {"type": "Adam", "betas": betas, "eps": eps}


def checkpoint_optimizer(checkpoint):
    actual = optimizer_configuration(checkpoint["optimizer"])
    saved = checkpoint.get("optimizer_config")
    if saved is not None:
        saved = {
            "type": saved.get("type"), "betas": tuple(saved.get("betas", ())),
            "eps": float(saved.get("eps", math.nan)),
        }
        if saved != actual:
            raise ValueError("checkpoint optimizer metadata does not match its state")
    checkpoint["optimizer_config"] = actual
    return actual


def require_optimizer_config(checkpoint):
    actual = checkpoint_optimizer(checkpoint)
    if actual != OPTIMIZER_CONFIG:
        raise ValueError(
            f"checkpoint optimizer {actual} does not match configured optimizer {OPTIMIZER_CONFIG}"
        )


def save_checkpoint(path, model, optimizer, manifest, stage, decisions, sampler_session,
                    promotion_index, progress_active, reservoir, auxiliary_decisions,
                    stage_decisions, critic_balance=None, replace=False, revisions=None,
                    training_elapsed_seconds=0, checkpoint_id=None):
    if path.exists() and not replace:
        raise FileExistsError(path)
    target = path.with_suffix(path.suffix + ".tmp") if replace else path
    with target.open("wb" if replace else "xb") as output:
        torch.save({
            "schema": 1, "model_version": model.model_version, "feature_version": FEATURE_VERSION,
            "fingerprint": manifest["fingerprint"], "precision": manifest["precision"],
            "run_id": manifest.get("run_id"), "lineage_id": manifest.get("lineage_id"),
            "session_id": manifest.get("sessions", [{}])[-1].get("id"),
            "checkpoint_id": checkpoint_id,
            "layout": model.layout, "architecture": manifest["architecture"],
            "stage": stage, "decisions": decisions, "sampler_session": sampler_session,
            "promotion_index": promotion_index,
            "progress_active": progress_active,
            "auxiliary_decisions": auxiliary_decisions,
            "stage_decisions": stage_decisions,
            "critic_balance": critic_balance.state_dict() if critic_balance else None,
            "revisions": dict(revisions or {}),
            "training_elapsed_seconds": training_elapsed_seconds,
            "winning_reservoir": reservoir.state_dict() if reservoir else None,
            "model": model.state_dict(), "optimizer": optimizer.state_dict(),
            "optimizer_config": optimizer_configuration(optimizer.state_dict()),
            "torch_rng": torch.get_rng_state(),
        }, output)
        output.flush(); os.fsync(output.fileno())
    if replace:
        target.replace(path)
    return sha256_file(path)


def load(path, target):
    path = Path(path)
    metadata_path = path.parent / "latest.json"
    if path.name == "latest.pt" and metadata_path.exists():
        metadata = json.loads(metadata_path.read_text())
        committed = path.parent / metadata.get("checkpoint", path.name)
        if not committed.exists() or sha256_file(committed) != metadata.get("sha256"):
            raise ValueError("committed checkpoint is missing or corrupt")
        if committed != path:
            if not path.exists() or sha256_file(path) != metadata["sha256"]:
                link_checkpoint(committed, path)
            path = committed
    checkpoint = torch.load(path, map_location=target, weights_only=False)
    version = checkpoint.get("model_version")
    if checkpoint.get("schema") != 1 or version not in COMPATIBLE_MODEL_VERSIONS:
        raise ValueError("incompatible checkpoint")
    config = checkpoint.get("architecture", {})
    try:
        dimensions = tuple(config[key] for key in ("width", "layers", "heads", "feedforward"))
        live = sts2_sim.Batch(1, 0, None, ascension=0)
        layout = dict(live.token_layout(*dimensions))
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError("invalid checkpoint architecture") from error
    if checkpoint.get("feature_version") != FEATURE_VERSION or layout["version"] != FEATURE_VERSION:
        raise ValueError("incompatible feature version")
    if checkpoint["fingerprint"] != live.fingerprint() or checkpoint["layout"] != layout:
        raise ValueError("checkpoint does not match simulator content")
    try:
        model = Agent(layout, *(config[key] for key in (
            "width", "layers", "heads", "feedforward", "head_width"
        )), pooling=config["pooling"], model_version=version,
                      potential_weights=config["potential_weights"]).to(target)
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError("invalid checkpoint architecture") from error
    if config != architecture(model):
        raise ValueError("incompatible model architecture")
    state = checkpoint["model"]
    missing, unexpected = model.load_state_dict(state, strict=True)
    if missing or unexpected:
        raise ValueError("incompatible model parameters")
    checkpoint_optimizer(checkpoint)
    checkpoint["_optimizer_compatible"] = optimizer_compatible(
        checkpoint["optimizer"], len(tuple(model.parameters()))
    )
    checkpoint["architecture"] = architecture(model)
    return model, checkpoint


def export_value_model(path, model, fingerprint, temperature, bias, actor=False):
    path = Path(path) if path is not None else None
    if path is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
    parts = [
        b"STSVALUE", struct.pack("<IIQ", model.model_version, FEATURE_VERSION, fingerprint),
        struct.pack("<10I", model.width, model.layers, model.heads, model.feedforward,
                    len(TOKEN_SPECS), len(SEMANTIC_NAMES), model.concepts.num_embeddings,
                    ACTION_FIELDS[2], ACTION_FIELDS[3], 1),
        struct.pack("<64I", *(value for _name, unsigned, signed, semantic, numeric in TOKEN_SPECS
                              for value in (unsigned, signed, semantic, numeric))),
        struct.pack(f"<{len(SEMANTIC_NAMES)}I", *model.concepts.sizes),
        struct.pack(f"<{len(POSITION_CAPS)}I", *POSITION_CAPS.values()),
        bytes([
            (COLLECTION_POOLING.index(mode) if index < 8 else
             EFFECT_POOLING.index(mode) if index < 10 else
             CONTINUATION_POOLING.index(mode) if index == 10 else
             ("sum", "transformer").index(mode))
            for index, mode in enumerate(model.pooling.values())
        ]),
        bytes([actor]),
        struct.pack("<2f", temperature, bias),
        struct.pack(f"<{len(POTENTIAL_TERMS)}f", *model.potential_weights),
    ]

    values = []

    def add(value):
        values.append(value.detach().reshape(-1))

    def flush():
        parts.append(torch.cat(values).float().cpu().numpy().astype("<f4", copy=False).tobytes())
        values.clear()

    def add_linear(module):
        add(module.weight)
        if module.bias is not None:
            add(module.bias)

    def add_norm(module):
        add(module.weight); add(module.bias)

    def add_encoder(module):
        add(module.numeric.weight); add_norm(module.norm)

    def add_transformer(transformer):
        for layer in transformer.layers:
            for value in (
                layer.self_attn.in_proj_weight, layer.self_attn.in_proj_bias,
                layer.self_attn.out_proj.weight, layer.self_attn.out_proj.bias,
                layer.norm1.weight, layer.norm1.bias, layer.norm2.weight, layer.norm2.bias,
                layer.linear1.weight, layer.linear1.bias, layer.linear2.weight, layer.linear2.bias,
            ):
                add(value)

    def add_gru(gru):
        add(gru.weight_ih_l0); add(gru.bias_ih_l0)
        add(gru.weight_hh_l0); add(gru.bias_hh_l0)

    add(model.concepts.flattened())
    for name, *_shape in TOKEN_SPECS:
        add_encoder(model.encoders[name])
    add_encoder(model.action_encoder)
    for name in (*POOLING_DEFAULTS, "card_pool", "relic_pool", "encounter_pool", "event_pool"):
        add(model.summary_seed[name])
    for index, (name, mode) in enumerate(model.pooling.items()):
        if (index < 8 and mode == "transformer") \
                or (index < 10 and mode.startswith("transformer")) \
                or (index == 10 and mode == "transformer") \
                or (index >= 11 and mode == "transformer"):
            add_transformer(model.pool_transformers[name])
    add_gru(model.move_gru)
    if model.continuation_gru is not None:
        add_gru(model.continuation_gru)
    add_norm(model.actor_norm); add_norm(model.action_norm)
    add_transformer(model.global_transformer); add_norm(model.global_norm)
    add_norm(model.graph_norm)
    for module in (model.graph_query, model.graph_key_value, model.graph_edge, model.graph_out,
                   model.graph_degree):
        add_linear(module)
    add_norm(model.graph_ff_norm); add_linear(model.graph_ff[0]); add_linear(model.graph_ff[2])
    if actor:
        add_linear(model.policy)
    add_linear(model.critic)
    flush()
    data = b"".join(parts)
    if path is None:
        return data
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_bytes(data); temporary.replace(path)
    return sha256_file(path)
