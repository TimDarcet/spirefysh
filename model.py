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
constant uint TOKEN_REDUCTION = 128;

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

kernel void attention_forward_bfloat_x4(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device const int *selected, device bfloat *output, device float *lse,
    constant uint& queries, constant uint& heads, constant uint& dimension,
    constant bool& sparse,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint first = tid / SIMD_WIDTH * 4, total = queries * heads, width = heads * dimension;
    uint first_head = first / queries, first_query = first % queries;
    bool shared = first + 3 < total && (first + 3) / queries == first_head;
    uint tokens[4];
    int rows[4];
    for (uint slot = 0; slot < 4 && shared; ++slot) {
        uint query_index = first_query + slot;
        tokens[slot] = sparse ? selected[query_index] : query_index;
        rows[slot] = sequence[tokens[slot]];
        shared &= rows[slot] == rows[0];
    }
    if (shared) {
        int begin = offsets[rows[0]], end = offsets[rows[0] + 1];
        float q[4], maximum[4], sum[4] = {0}, values[4] = {0};
        for (uint slot = 0; slot < 4; ++slot) {
            uint query = tokens[slot] * 3 * width + first_head * dimension + lane;
            q[slot] = float(qkv[query]);
            maximum[slot] = -INFINITY;
        }
        for (int key = begin; key < end; ++key) {
            uint k = key * 3 * width + width + first_head * dimension + lane;
            float key_value = float(qkv[k]), value = float(qkv[k + width]);
            for (uint slot = 0; slot < 4; ++slot) {
                float score = head_sum(q[slot] * key_value, dimension, lane)
                    * rsqrt(float(dimension));
                float next = max(maximum[slot], score);
                float old = exp(maximum[slot] - next), weight = exp(score - next);
                sum[slot] = sum[slot] * old + weight;
                values[slot] = values[slot] * old + weight * value;
                maximum[slot] = next;
            }
        }
        for (uint slot = 0; slot < 4; ++slot) {
            uint out = (first_query + slot) * width + first_head * dimension + lane;
            output[out] = bfloat(values[slot] / sum[slot]);
            if (lane == 0)
                lse[(first_query + slot) * heads + first_head] = maximum[slot] + log(sum[slot]);
        }
        return;
    }
    for (uint logical = first; logical < min(first + 4, total); ++logical) {
        uint head = logical / queries, query_index = logical % queries;
        uint token = sparse ? selected[query_index] : query_index;
        int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
        uint query = token * 3 * width + head * dimension + lane;
        uint out = query_index * width + head * dimension + lane;
        if (end == begin + 1) {
            output[out] = qkv[query + 2 * width];
            if (lane == 0) lse[query_index * heads + head] = 0.0f;
            continue;
        }
        float q = float(qkv[query]), maximum = -INFINITY, sum = 0.0f, value = 0.0f;
        for (int key = begin; key < end; ++key) {
            uint k = key * 3 * width + width + head * dimension;
            float score = head_sum(q * float(qkv[k + lane]), dimension, lane)
                * rsqrt(float(dimension));
            float next = max(maximum, score), old = exp(maximum - next);
            float weight = exp(score - next);
            sum = sum * old + weight;
            value = value * old + weight * float(qkv[k + width + lane]);
            maximum = next;
        }
        output[out] = bfloat(value / sum);
        if (lane == 0) lse[query_index * heads + head] = maximum + log(sum);
    }
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

kernel void attention_backward_query_bfloat_x4(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device const int *selected, device const bfloat *output, device const float *lse,
    device const bfloat *grad_output, device bfloat *grad_qkv, device float *delta,
    constant uint& queries, constant uint& heads, constant uint& dimension,
    constant bool& sparse,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint first = tid / SIMD_WIDTH * 4, total = queries * heads, width = heads * dimension;
    uint first_head = first / queries, first_query = first % queries;
    bool shared = first + 3 < total && (first + 3) / queries == first_head;
    uint tokens[4];
    int rows[4];
    for (uint slot = 0; slot < 4 && shared; ++slot) {
        uint query_index = first_query + slot;
        tokens[slot] = sparse ? selected[query_index] : query_index;
        rows[slot] = sequence[tokens[slot]];
        shared &= rows[slot] == rows[0];
    }
    if (shared) {
        int begin = offsets[rows[0]], end = offsets[rows[0] + 1];
        float q[4], grad[4], correction[4], dq[4] = {0};
        for (uint slot = 0; slot < 4; ++slot) {
            uint query = tokens[slot] * 3 * width + first_head * dimension + lane;
            uint out = (first_query + slot) * width + first_head * dimension + lane;
            q[slot] = float(qkv[query]);
            grad[slot] = float(grad_output[out]);
            correction[slot] = head_sum(grad[slot] * float(output[out]), dimension, lane);
        }
        for (int key = begin; key < end; ++key) {
            uint k = key * 3 * width + width + first_head * dimension + lane;
            float key_value = float(qkv[k]), value = float(qkv[k + width]);
            for (uint slot = 0; slot < 4; ++slot) {
                float probability = exp(head_sum(q[slot] * key_value, dimension, lane)
                    * rsqrt(float(dimension))
                    - lse[(first_query + slot) * heads + first_head]);
                float dp = head_sum(grad[slot] * value, dimension, lane);
                dq[slot] += probability * (dp - correction[slot]) * key_value
                    * rsqrt(float(dimension));
            }
        }
        for (uint slot = 0; slot < 4; ++slot) {
            uint query = tokens[slot] * 3 * width + first_head * dimension + lane;
            grad_qkv[query] = bfloat(dq[slot]);
            if (lane == 0)
                delta[(first_query + slot) * heads + first_head] = correction[slot];
        }
        return;
    }
    for (uint logical = first; logical < min(first + 4, total); ++logical) {
        uint head = logical / queries, query_index = logical % queries;
        uint token = sparse ? selected[query_index] : query_index;
        int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
        uint query = token * 3 * width + head * dimension + lane;
        uint out = query_index * width + head * dimension + lane;
        if (end == begin + 1) {
            grad_qkv[query] = bfloat(0.0f);
            if (lane == 0) delta[query_index * heads + head] = 0.0f;
            continue;
        }
        float q = float(qkv[query]), grad = float(grad_output[out]);
        float correction = head_sum(grad * float(output[out]), dimension, lane), dq = 0.0f;
        for (int key = begin; key < end; ++key) {
            uint k = key * 3 * width + width + head * dimension + lane;
            float key_value = float(qkv[k]);
            float probability = exp(head_sum(q * key_value, dimension, lane)
                * rsqrt(float(dimension)) - lse[query_index * heads + head]);
            float dp = head_sum(grad * float(qkv[k + width]), dimension, lane);
            dq += probability * (dp - correction) * key_value * rsqrt(float(dimension));
        }
        grad_qkv[query] = bfloat(dq);
        if (lane == 0) delta[query_index * heads + head] = correction;
    }
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

kernel void attention_backward_key_value_bfloat_x4(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device const int *selected, device const int *query_offsets,
    device const bfloat *output, device const float *lse, device const bfloat *grad_output,
    device const float *delta, device bfloat *grad_qkv, constant uint& keys,
    constant uint& heads, constant uint& dimension, constant bool& sparse,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint first = tid / SIMD_WIDTH * 4, total = keys * heads, width = heads * dimension;
    uint first_head = first / keys, first_key = first % keys;
    bool shared = first + 3 < total && (first + 3) / keys == first_head;
    int rows[4];
    for (uint slot = 0; slot < 4 && shared; ++slot) {
        rows[slot] = sequence[first_key + slot];
        shared &= rows[slot] == rows[0];
    }
    if (shared) {
        int begin = sparse ? query_offsets[rows[0]] : offsets[rows[0]];
        int end = sparse ? query_offsets[rows[0] + 1] : offsets[rows[0] + 1];
        float key_values[4], values[4], dk[4] = {0}, dv[4] = {0};
        bool selected_queries[4] = {false};
        for (uint slot = 0; slot < 4; ++slot) {
            uint base = (first_key + slot) * 3 * width + width
                + first_head * dimension + lane;
            key_values[slot] = float(qkv[base]);
            values[slot] = float(qkv[base + width]);
        }
        for (int query_index = begin; query_index < end; ++query_index) {
            uint token = sparse ? selected[query_index] : query_index;
            uint query = token * 3 * width + first_head * dimension + lane;
            uint out = query_index * width + first_head * dimension + lane;
            float q = float(qkv[query]), grad = float(grad_output[out]);
            for (uint slot = 0; slot < 4; ++slot) {
                selected_queries[slot] |= token == first_key + slot;
                float probability = exp(head_sum(q * key_values[slot], dimension, lane)
                    * rsqrt(float(dimension)) - lse[query_index * heads + first_head]);
                float ds = probability * (head_sum(grad * values[slot], dimension, lane)
                    - delta[query_index * heads + first_head]);
                dk[slot] += ds * q * rsqrt(float(dimension));
                dv[slot] += probability * grad;
            }
        }
        for (uint slot = 0; slot < 4; ++slot) {
            uint key = first_key + slot;
            uint base = key * 3 * width + width + first_head * dimension + lane;
            if (sparse && !selected_queries[slot])
                grad_qkv[key * 3 * width + first_head * dimension + lane] = bfloat(0.0f);
            grad_qkv[base] = bfloat(dk[slot]);
            grad_qkv[base + width] = bfloat(dv[slot]);
        }
        return;
    }
    for (uint logical = first; logical < min(first + 4, total); ++logical) {
        uint head = logical / keys, key = logical % keys;
        int row = sequence[key], begin = sparse ? query_offsets[row] : offsets[row];
        int end = sparse ? query_offsets[row + 1] : offsets[row + 1];
        uint base = key * 3 * width + width + head * dimension + lane;
        if (offsets[row + 1] == offsets[row] + 1) {
            grad_qkv[base] = bfloat(0.0f);
            grad_qkv[base + width] = grad_output[begin * width + head * dimension + lane];
            continue;
        }
        bool selected_query = false;
        float key_value = float(qkv[base]), value = float(qkv[base + width]);
        float dk = 0.0f, dv = 0.0f;
        for (int query_index = begin; query_index < end; ++query_index) {
            uint token = sparse ? selected[query_index] : query_index;
            if (sparse) selected_query |= token == key;
            uint query = token * 3 * width + head * dimension + lane;
            uint out = query_index * width + head * dimension + lane;
            float q = float(qkv[query]), grad = float(grad_output[out]);
            float probability = exp(head_sum(q * key_value, dimension, lane)
                * rsqrt(float(dimension)) - lse[query_index * heads + head]);
            float ds = probability * (head_sum(grad * value, dimension, lane)
                - delta[query_index * heads + head]);
            dk += ds * q * rsqrt(float(dimension));
            dv += probability * grad;
        }
        if (sparse && !selected_query)
            grad_qkv[key * 3 * width + head * dimension + lane] = bfloat(0.0f);
        grad_qkv[base] = bfloat(dk);
        grad_qkv[base + width] = bfloat(dv);
    }
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

kernel void semantic_forward_bfloat(
    device const int *semantic, device const bfloat *numeric, device const float *embedding,
    device const float *gamma, device const float *beta, device float *input,
    device float *output, device float *stats, constant uint& rows, constant uint& fields,
    constant uint& width,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint row = tid / 32, base = row * width + lane;
    float4 value;
    for (uint part = 0; part < width / 32; ++part) {
        uint column = lane + part * 32;
        value[part] = float(numeric[row * width + column]);
        for (uint position = 0; position < fields; ++position)
            value[part] += embedding[semantic[row * fields + position] * width + column];
    }
    float local_sum = 0.0f, local_square = 0.0f;
    for (uint part = 0; part < width / 32; ++part) {
        local_sum += value[part];
        local_square += value[part] * value[part];
    }
    float sum = simd_sum(local_sum), square = simd_sum(local_square);
    float mean = sum / width;
    float inverse_std = rsqrt(max(0.0f, square / width - mean * mean) + 1e-5f);
    for (uint part = 0; part < width / 32; ++part) {
        uint column = lane + part * 32, index = base + part * 32;
        input[index] = value[part];
        output[index] = (value[part] - mean) * inverse_std * gamma[column] + beta[column];
    }
    if (lane == 0) {
        stats[row * 2] = mean;
        stats[row * 2 + 1] = inverse_std;
    }
}

kernel void semantic_backward_input(
    device const float *input, device const float *gamma, device const float *stats,
    device const float *grad_output, device float *grad_input, constant uint& width,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint row = tid / 32, base = row * width + lane;
    float mean = stats[row * 2], inverse_std = stats[row * 2 + 1];
    float4 normalized, grad;
    for (uint part = 0; part < width / 32; ++part) {
        uint column = lane + part * 32, index = base + part * 32;
        normalized[part] = (input[index] - mean) * inverse_std;
        grad[part] = grad_output[index] * gamma[column];
    }
    float local_sum = 0.0f, local_product = 0.0f;
    for (uint part = 0; part < width / 32; ++part) {
        local_sum += grad[part];
        local_product += grad[part] * normalized[part];
    }
    float sum = simd_sum(local_sum), product = simd_sum(local_product);
    for (uint part = 0; part < width / 32; ++part)
        grad_input[base + part * 32] = (
            grad[part] - sum / width - normalized[part] * product / width
        ) * inverse_std;
}

kernel void semantic_backward_norm_partial(
    device const float *input, device const float *stats, device const float *grad_output,
    device float *partial, constant uint& rows, constant uint& block_rows, constant uint& width,
    uint tid [[thread_position_in_grid]]) {
    uint block = tid / width, column = tid % width;
    uint begin = block * block_rows, end = min(rows, begin + block_rows);
    float dg = 0.0f, db = 0.0f;
    for (uint row = begin; row < end; ++row) {
        uint index = row * width + column;
        float normalized = (input[index] - stats[row * 2]) * stats[row * 2 + 1];
        dg += grad_output[index] * normalized;
        db += grad_output[index];
    }
    partial[block * 2 * width + column] = dg;
    partial[(block * 2 + 1) * width + column] = db;
}

kernel void token_backward_norm_reduce(
    device const float *partial, device float *grad_gamma, device float *grad_beta,
    constant uint& blocks, constant uint& width,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_threadgroup]]) {
    threadgroup float gamma_sum[TOKEN_REDUCTION], beta_sum[TOKEN_REDUCTION];
    uint column = tid / TOKEN_REDUCTION;
    float dg = 0.0f, db = 0.0f;
    for (uint block = lane; block < blocks; block += TOKEN_REDUCTION) {
        dg += partial[block * 2 * width + column];
        db += partial[(block * 2 + 1) * width + column];
    }
    gamma_sum[lane] = dg;
    beta_sum[lane] = db;
    for (uint stride = TOKEN_REDUCTION / 2; stride; stride /= 2) {
        threadgroup_barrier(mem_flags::mem_threadgroup);
        if (lane < stride) {
            gamma_sum[lane] += gamma_sum[lane + stride];
            beta_sum[lane] += beta_sum[lane + stride];
        }
    }
    if (lane == 0) {
        grad_gamma[column] = gamma_sum[0];
        grad_beta[column] = beta_sum[0];
    }
}


kernel void semantic_backward_embedding(
    device const int *semantic, device const float *grad_input,
    device atomic_float *grad_embedding, constant uint& rows, constant uint& fields,
    constant uint& width, uint index [[thread_position_in_grid]]) {
    uint row = index / width, column = index % width;
    float grad = grad_input[index];
    for (uint field = 0; field < fields; ++field) {
        int id = semantic[row * fields + field];
        if (id) atomic_fetch_add_explicit(
            grad_embedding + id * width + column, grad, memory_order_relaxed
        );
    }
}

kernel void group_sum_forward(
    device const float *input, device const long *group, device atomic_float *output,
    constant uint& width, uint index [[thread_position_in_grid]]) {
    atomic_fetch_add_explicit(
        output + group[index / width] * width + index % width,
        input[index], memory_order_relaxed
    );
}

kernel void group_sum_backward(
    device const float *grad_output, device const long *group, device float *grad_input,
    constant uint& width, uint index [[thread_position_in_grid]]) {
    grad_input[index] = grad_output[group[index / width] * width + index % width];
}

kernel void indexed_group_sum_forward(
    device const float *input, device const long *source, device const long *group,
    device atomic_float *output, constant uint& width,
    uint index [[thread_position_in_grid]]) {
    uint item = index / width, column = index % width;
    atomic_fetch_add_explicit(
        output + group[item] * width + column,
        input[source[item] * width + column], memory_order_relaxed
    );
}

kernel void indexed_group_sum_backward(
    device const float *grad_output, device const long *source, device const long *group,
    device atomic_float *grad_input, constant uint& width,
    uint index [[thread_position_in_grid]]) {
    uint item = index / width, column = index % width;
    atomic_fetch_add_explicit(
        grad_input + source[item] * width + column,
        grad_output[group[item] * width + column], memory_order_relaxed
    );
}

kernel void indexed_scatter_add(
    device const float *input, device const long *source, device const long *target,
    device atomic_float *output, constant uint& offset, constant uint& width,
    uint index [[thread_position_in_grid]]) {
    uint item = index / width, column = index % width;
    atomic_fetch_add_explicit(
        output + target[offset + item] * width + column,
        input[source[item] * width + column], memory_order_relaxed
    );
}

kernel void indexed_scatter_backward(
    device const float *grad_output, device const long *source, device const long *target,
    device atomic_float *grad_input, constant uint& offset, constant uint& width,
    uint index [[thread_position_in_grid]]) {
    uint item = index / width, column = index % width;
    atomic_fetch_add_explicit(
        grad_input + source[item] * width + column,
        grad_output[target[offset + item] * width + column], memory_order_relaxed
    );
}

kernel void action_nodes_forward(
    device const float *nodes, device const long *path, device float *output,
    constant uint& width, uint index [[thread_position_in_grid]]) {
    uint action = index / width;
    if (path[action] >= 0)
        output[index] += nodes[path[action] * width + index % width];
}

kernel void action_nodes_backward(
    device const float *grad_output, device const long *path, device atomic_float *grad_nodes,
    constant uint& width, uint index [[thread_position_in_grid]]) {
    uint action = index / width;
    if (path[action] >= 0) atomic_fetch_add_explicit(
        grad_nodes + path[action] * width + index % width,
        grad_output[index], memory_order_relaxed
    );
}

kernel void layer_norm_forward_bfloat(
    device const float *input, device const float *gamma, device const float *beta,
    device bfloat *output, device float *stats, constant uint& rows, constant uint& width,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint row = tid / 32, base = row * width + lane;
    float4 value;
    float local_sum = 0.0f, local_square = 0.0f;
    for (uint part = 0; part < width / 32; ++part) {
        value[part] = input[base + part * 32];
        local_sum += value[part];
        local_square += value[part] * value[part];
    }
    float sum = simd_sum(local_sum), square = simd_sum(local_square);
    float mean = sum / width;
    float inverse_std = rsqrt(max(0.0f, square / width - mean * mean) + 1e-5f);
    for (uint part = 0; part < width / 32; ++part) {
        uint column = lane + part * 32;
        output[base + part * 32] = bfloat(
            (value[part] - mean) * inverse_std * gamma[column] + beta[column]
        );
    }
    if (lane == 0) {
        stats[row * 2] = mean;
        stats[row * 2 + 1] = inverse_std;
    }
}

kernel void layer_norm_backward_input_bfloat(
    device const float *input, device const float *gamma, device const float *stats,
    device const bfloat *grad_output, device float *grad_input, constant uint& width,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint row = tid / 32, base = row * width + lane;
    float mean = stats[row * 2], inverse_std = stats[row * 2 + 1];
    float4 normalized, grad;
    float local_sum = 0.0f, local_product = 0.0f;
    for (uint part = 0; part < width / 32; ++part) {
        uint column = lane + part * 32, index = base + part * 32;
        normalized[part] = (input[index] - mean) * inverse_std;
        grad[part] = float(grad_output[index]) * gamma[column];
        local_sum += grad[part];
        local_product += grad[part] * normalized[part];
    }
    float sum = simd_sum(local_sum), product = simd_sum(local_product);
    for (uint part = 0; part < width / 32; ++part)
        grad_input[base + part * 32] = (
            grad[part] - sum / width - normalized[part] * product / width
        ) * inverse_std;
}

kernel void layer_norm_backward_partial_bfloat(
    device const float *input, device const float *stats, device const bfloat *grad_output,
    device float *partial, constant uint& rows, constant uint& block_rows, constant uint& width,
    uint tid [[thread_position_in_grid]]) {
    uint block = tid / width, column = tid % width;
    uint begin = block * block_rows, end = min(rows, begin + block_rows);
    float dg = 0.0f, db = 0.0f;
    for (uint row = begin; row < end; ++row) {
        uint index = row * width + column;
        float grad = float(grad_output[index]);
        dg += grad * (input[index] - stats[row * 2]) * stats[row * 2 + 1];
        db += grad;
    }
    partial[block * 2 * width + column] = dg;
    partial[(block * 2 + 1) * width + column] = db;
}

kernel void norm_head_forward(
    device const float *input, device const float *gamma, device const float *beta,
    device const float *weight, device const float *bias, device bfloat *normalized,
    device bfloat *output, device float *stats, constant uint& width,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint row = tid / 32, base = row * width + lane;
    float4 value;
    float local_sum = 0.0f, local_square = 0.0f;
    for (uint part = 0; part < width / 32; ++part) {
        value[part] = input[base + part * 32];
        local_sum += value[part];
        local_square += value[part] * value[part];
    }
    float sum = simd_sum(local_sum), square = simd_sum(local_square);
    float mean = sum / width;
    float inverse_std = rsqrt(max(0.0f, square / width - mean * mean) + 1e-5f);
    float dot = 0.0f;
    for (uint part = 0; part < width / 32; ++part) {
        uint column = lane + part * 32, index = base + part * 32;
        bfloat item = bfloat((value[part] - mean) * inverse_std
                             * gamma[column] + beta[column]);
        normalized[index] = item;
        dot += float(item) * float(bfloat(weight[column]));
    }
    dot = simd_sum(dot);
    if (lane == 0) {
        output[row] = bfloat(dot + float(bfloat(bias[0])));
        stats[row * 2] = mean;
        stats[row * 2 + 1] = inverse_std;
    }
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
        suffix = "_half" if qkv.dtype == torch.float16 else "_bfloat" if qkv.dtype == torch.bfloat16 else ""
        tiled = bool(suffix) and dimension == 32
        threads = ((queries * heads + 3) // 4 * 32 if tiled
                   else queries * heads * dimension)
        kernel = getattr(library, f"attention_forward{suffix}{'_x4' if tiled else ''}")
        arguments = qkv, offsets, sequence, selected, output, lse
        if tiled:
            kernel(
                *arguments, queries, heads, dimension, sparse,
                threads=threads, group_size=32,
            )
        else:
            kernel(
                *arguments, heads, dimension, sparse, threads=threads, group_size=32,
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
        tiled = bool(ctx.suffix) and ctx.dimension == 32
        query_threads = ((len(output) * ctx.heads + 3) // 4 * 32 if tiled
                         else len(output) * ctx.heads * ctx.dimension)
        key_threads = ((len(qkv) * ctx.heads + 3) // 4 * 32 if tiled
                       else len(qkv) * ctx.heads * ctx.dimension)
        args = qkv, offsets, sequence, selected, output, lse, grad_output.contiguous()
        query = getattr(
            ctx.library, f"attention_backward_query{ctx.suffix}{'_x4' if tiled else ''}",
        ) if ctx.suffix else ctx.library.backward_query
        key_value = getattr(
            ctx.library,
            f"attention_backward_key_value{ctx.suffix}{'_x4' if tiled else ''}",
        ) if ctx.suffix else ctx.library.backward_key_value
        if tiled:
            query(
                *args, grad_qkv, delta, len(output), ctx.heads, ctx.dimension, ctx.sparse,
                threads=query_threads, group_size=32,
            )
        else:
            query(
                *args, grad_qkv, delta, ctx.heads, ctx.dimension, ctx.sparse,
                threads=query_threads, group_size=32,
            )
        arguments = (
            qkv, offsets, sequence, selected, query_offsets, output, lse,
            grad_output.contiguous(), delta, grad_qkv,
        )
        if tiled:
            key_value(
                *arguments, len(qkv), ctx.heads, ctx.dimension, ctx.sparse,
                threads=key_threads, group_size=32,
            )
        else:
            key_value(
                *arguments, ctx.heads, ctx.dimension, ctx.sparse,
                threads=key_threads, group_size=32,
            )
        return grad_qkv, None, None, None, None, None, None


class _FlashAttention(torch.autograd.Function):
    library = None

    @staticmethod
    def forward(ctx, qkv, offsets, lengths, block_rows, block_starts):
        if _FlashAttention.library is None:
            _FlashAttention.library = torch.mps.compile_shader(
                Path(__file__).with_name("attention.metal").read_text()
            )
        output = qkv.new_empty((len(qkv), qkv.shape[1] // 3))
        lse = torch.empty((len(qkv), 4), dtype=torch.float32, device=qkv.device)
        threads = len(block_rows) * 4 * 64
        _FlashAttention.library.flash_forward(
            qkv, qkv, qkv, output, lse, offsets, lengths, block_rows, block_starts,
            threads=threads, group_size=64,
        )
        ctx.save_for_backward(qkv, output, lse, offsets, lengths, block_rows, block_starts)
        ctx.threads = threads
        return output

    @staticmethod
    def backward(ctx, grad_output):
        qkv, output, lse, offsets, lengths, block_rows, block_starts = ctx.saved_tensors
        grad_output = grad_output.contiguous()
        grad_qkv = torch.empty_like(qkv)
        delta = torch.empty_like(lse)
        _FlashAttention.library.flash_backward_query(
            qkv, qkv, qkv, output, lse, delta, grad_output, grad_qkv,
            offsets, lengths, block_rows, block_starts,
            threads=ctx.threads, group_size=64,
        )
        _FlashAttention.library.flash_backward_key_value(
            qkv, qkv, qkv, lse, delta, grad_output, grad_qkv, grad_qkv,
            offsets, lengths, block_rows, block_starts,
            threads=ctx.threads, group_size=64,
        )
        return grad_qkv, None, None, None, None


def _linear_forward(value_bfloat, weight, bias, residual=None):
    if _FastLinear.libraries is None:
        sources = Path(__file__).with_name("linear.metal").read_text().split(
            "// SPIREFYSH_KERNEL"
        )
        _FastLinear.libraries = [torch.mps.compile_shader(source) for source in sources]
    weight_bfloat = weight.to(torch.bfloat16)
    output = value_bfloat.new_empty(
        (len(value_bfloat), len(weight)), dtype=residual.dtype if residual is not None else None,
    )
    name = "linear_residual_128_128" if residual is not None else \
        f"linear_{value_bfloat.shape[1]}_{len(weight)}"
    arguments = (value_bfloat, weight_bfloat, output, bias.to(torch.bfloat16))
    getattr(_FastLinear.libraries[2 if residual is not None else len(weight) != 128], name)(
        *arguments, *((residual,) if residual is not None else ()), len(value_bfloat),
        threads=((len(weight) + 31) // 32 * 32, (len(value_bfloat) + 31) // 32),
        group_size=(32, 1),
    )
    return output, weight_bfloat


class _FastLinear(torch.autograd.Function):
    libraries = None

    @staticmethod
    def forward(ctx, value, weight, bias):
        value_bfloat = value.to(torch.bfloat16)
        output, weight_bfloat = _linear_forward(value_bfloat, weight, bias)
        ctx.save_for_backward(value_bfloat, weight_bfloat)
        ctx.value_dtype = value.dtype
        return output

    @staticmethod
    def backward(ctx, grad_output):
        value, weight = ctx.saved_tensors
        grad_value, grad_weight, grad_bias = torch.ops.aten.linear_backward(
            value, grad_output.to(torch.bfloat16), weight, [True, True, True],
        )
        return grad_value.to(ctx.value_dtype), grad_weight.float(), grad_bias.float()


class _LinearResidual(torch.autograd.Function):
    @staticmethod
    def forward(ctx, value, weight, bias, residual):
        value_bfloat = value.to(torch.bfloat16)
        output, weight_bfloat = _linear_forward(value_bfloat, weight, bias, residual)
        ctx.save_for_backward(value_bfloat, weight_bfloat)
        ctx.value_dtype = value.dtype
        return output

    @staticmethod
    def backward(ctx, grad_output):
        value, weight = ctx.saved_tensors
        grad_value, grad_weight, grad_bias = torch.ops.aten.linear_backward(
            value, grad_output.to(torch.bfloat16), weight, [True, True, True],
        )
        return grad_value.to(ctx.value_dtype), grad_weight.float(), grad_bias.float(), grad_output


class _NormLinear(torch.autograd.Function):
    @staticmethod
    def forward(ctx, value, gamma, beta, weight, bias):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        normalized = torch.empty_like(value, dtype=torch.bfloat16)
        stats = value.new_empty((len(value), 2))
        _RaggedAttention.library.layer_norm_forward_bfloat(
            value, gamma, beta, normalized, stats, len(value), value.shape[1],
            threads=len(value) * 32, group_size=32,
        )
        output, weight_bfloat = _linear_forward(normalized, weight, bias)
        ctx.save_for_backward(value, gamma, normalized, stats, weight_bfloat)
        return output

    @staticmethod
    def backward(ctx, grad_output):
        value, gamma, normalized, stats, weight = ctx.saved_tensors
        grad_normalized, grad_weight, grad_bias = torch.ops.aten.linear_backward(
            normalized, grad_output.to(torch.bfloat16), weight, [True, True, True],
        )
        width = value.shape[1]
        grad_value = torch.empty_like(value)
        _RaggedAttention.library.layer_norm_backward_input_bfloat(
            value, gamma, stats, grad_normalized, grad_value, width,
            threads=len(value) * 32, group_size=32,
        )
        blocks = (len(value) + 127) // 128
        partial = value.new_empty((blocks, 2, width))
        grad_gamma, grad_beta = torch.empty_like(gamma), torch.empty_like(gamma)
        _RaggedAttention.library.layer_norm_backward_partial_bfloat(
            value, stats, grad_normalized, partial, len(value), 128, width,
            threads=blocks * width, group_size=128,
        )
        _RaggedAttention.library.token_backward_norm_reduce(
            partial, grad_gamma, grad_beta, blocks, width,
            threads=width * 128, group_size=128,
        )
        return grad_value, grad_gamma, grad_beta, grad_weight.float(), grad_bias.float()


class _NormHead(torch.autograd.Function):
    @staticmethod
    def forward(ctx, value, gamma, beta, weight, bias):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        normalized = torch.empty_like(value, dtype=torch.bfloat16)
        output = torch.empty(len(value), dtype=torch.bfloat16, device=value.device)
        stats = value.new_empty((len(value), 2))
        _RaggedAttention.library.norm_head_forward(
            value, gamma, beta, weight, bias, normalized, output, stats, value.shape[1],
            threads=len(value) * 32, group_size=32,
        )
        ctx.save_for_backward(value, gamma, normalized, stats, weight.to(torch.bfloat16))
        return output

    @staticmethod
    def backward(ctx, grad_output):
        value, gamma, normalized, stats, weight = ctx.saved_tensors
        grad_normalized, grad_weight, grad_bias = torch.ops.aten.linear_backward(
            normalized, grad_output[:, None].to(torch.bfloat16), weight, [True, True, True],
        )
        width = value.shape[1]
        grad_value = torch.empty_like(value)
        _RaggedAttention.library.layer_norm_backward_input_bfloat(
            value, gamma, stats, grad_normalized, grad_value, width,
            threads=len(value) * 32, group_size=32,
        )
        blocks = (len(value) + 127) // 128
        partial = value.new_empty((blocks, 2, width))
        grad_gamma, grad_beta = torch.empty_like(gamma), torch.empty_like(gamma)
        _RaggedAttention.library.layer_norm_backward_partial_bfloat(
            value, stats, grad_normalized, partial, len(value), 128, width,
            threads=blocks * width, group_size=128,
        )
        _RaggedAttention.library.token_backward_norm_reduce(
            partial, grad_gamma, grad_beta, blocks, width,
            threads=width * 128, group_size=128,
        )
        return grad_value, grad_gamma, grad_beta, grad_weight.float(), grad_bias.float()


def _linear(value, layer, residual=None):
    if (value.device.type == "mps" and torch.is_autocast_enabled("mps")
            and value.shape[1] == 128 and len(layer.weight) in (128, 384)
            and len(value) >= 4096):
        return (_FastLinear if residual is None else _LinearResidual).apply(
            value, layer.weight, layer.bias, *(() if residual is None else (residual,))
        )
    output = layer(value)
    return output if residual is None else residual + output


def _residual_linear(value, layer, residual):
    return _linear(value, layer, residual)


def _norm_linear(value, norm, weight, bias):
    if (value.device.type == "mps" and value.dtype == torch.float32
            and torch.is_autocast_enabled("mps") and value.shape[1] == 128
            and len(weight) in (128, 384) and len(value) >= 4096):
        return _NormLinear.apply(value, norm.weight, norm.bias, weight, bias)
    return nn.functional.linear(norm(value), weight, bias)


def _norm_head(value, norm, head):
    if (value.device.type == "mps" and value.dtype == torch.float32
            and torch.is_autocast_enabled("mps") and value.shape[1] == 128):
        return _NormHead.apply(value, norm.weight, norm.bias, head.weight, head.bias)
    return head(norm(value)).squeeze(-1)


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


class _FusedSemantic(torch.autograd.Function):
    @staticmethod
    def forward(ctx, semantic, numeric, embedding, gamma, beta):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        rows, fields = semantic.shape
        width = embedding.shape[1]
        input = embedding.new_empty((rows, width))
        output = torch.empty_like(input)
        stats = embedding.new_empty((rows, 2))
        _RaggedAttention.library.semantic_forward_bfloat(
            semantic, numeric, embedding, gamma, beta, input, output, stats, rows, fields, width,
            threads=rows * 32, group_size=32,
        )
        ctx.save_for_backward(semantic, embedding, gamma, input, stats)
        ctx.width = width
        return output

    @staticmethod
    def backward(ctx, grad_output):
        semantic, embedding, gamma, input, stats = ctx.saved_tensors
        grad_output = grad_output.contiguous()
        grad_input = torch.empty_like(input)
        _RaggedAttention.library.semantic_backward_input(
            input, gamma, stats, grad_output, grad_input,
            ctx.width, threads=len(input) * 32, group_size=32,
        )
        blocks = (len(input) + 127) // 128
        partial = torch.empty((blocks, 2, ctx.width), dtype=torch.float32, device=input.device)
        grad_gamma = torch.empty_like(gamma)
        grad_beta = torch.empty_like(gamma)
        _RaggedAttention.library.semantic_backward_norm_partial(
            input, stats, grad_output, partial, len(input), 128, ctx.width,
            threads=blocks * ctx.width, group_size=ctx.width,
        )
        _RaggedAttention.library.token_backward_norm_reduce(
            partial, grad_gamma, grad_beta, blocks, ctx.width,
            threads=ctx.width * 128, group_size=128,
        )
        grad_embedding = torch.zeros_like(embedding)
        _RaggedAttention.library.semantic_backward_embedding(
            semantic, grad_input, grad_embedding, len(input), semantic.shape[1], ctx.width,
            threads=len(input) * ctx.width, group_size=256,
        )
        return None, grad_input.to(torch.bfloat16), grad_embedding, grad_gamma, grad_beta


class _FusedDomains(torch.autograd.Function):
    @staticmethod
    def forward(ctx, embedding, *arguments):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        outputs, saved = [], [embedding]
        for categorical, numeric, weight, gamma, beta in zip(*[iter(arguments)] * 5):
            if not len(categorical):
                outputs.append(gamma.new_empty((0, len(gamma))))
                saved.extend((categorical, numeric, weight, gamma,
                              gamma.new_empty((0, len(gamma))), gamma.new_empty((0, 2))))
                continue
            numeric_input = numeric.to(torch.bfloat16)
            numeric = nn.functional.linear(
                numeric_input, weight.to(torch.bfloat16),
            )
            rows, fields = categorical.shape
            input = embedding.new_empty((rows, len(gamma)))
            output = torch.empty_like(input)
            stats = embedding.new_empty((rows, 2))
            _RaggedAttention.library.semantic_forward_bfloat(
                categorical, numeric, embedding, gamma, beta, input, output, stats,
                rows, fields, len(gamma), threads=rows * 32, group_size=32,
            )
            outputs.append(output)
            saved.extend((categorical, numeric_input, weight.to(torch.bfloat16), gamma,
                          input, stats))
        ctx.save_for_backward(*saved)
        return tuple(outputs)

    @staticmethod
    def backward(ctx, *grad_outputs):
        embedding, *saved = ctx.saved_tensors
        grad_embedding = torch.zeros_like(embedding)
        gradients = [grad_embedding]
        for grad_output, values in zip(grad_outputs, zip(*[iter(saved)] * 6)):
            semantic, numeric_input, weight, gamma, input, stats = values
            if not len(semantic):
                gradients.extend((None, None, torch.zeros_like(weight),
                                  torch.zeros_like(gamma), torch.zeros_like(gamma)))
                continue
            grad_output = grad_output.contiguous()
            grad_input = torch.empty_like(input)
            width = input.shape[1]
            _RaggedAttention.library.semantic_backward_input(
                input, gamma, stats, grad_output, grad_input, width,
                threads=len(input) * 32, group_size=32,
            )
            blocks = (len(input) + 127) // 128
            partial = input.new_empty((blocks, 2, width))
            grad_gamma, grad_beta = torch.empty_like(gamma), torch.empty_like(gamma)
            _RaggedAttention.library.semantic_backward_norm_partial(
                input, stats, grad_output, partial, len(input), 128, width,
                threads=blocks * width, group_size=width,
            )
            _RaggedAttention.library.token_backward_norm_reduce(
                partial, grad_gamma, grad_beta, blocks, width,
                threads=width * 128, group_size=128,
            )
            _RaggedAttention.library.semantic_backward_embedding(
                semantic, grad_input, grad_embedding, len(input), semantic.shape[1], width,
                threads=len(input) * width, group_size=256,
            )
            grad_weight = torch.ops.aten.linear_backward(
                numeric_input, grad_input.to(torch.bfloat16), weight, [False, True, False],
            )[1]
            gradients.extend((None, None, grad_weight.float(), grad_gamma, grad_beta))
        return tuple(gradients)


class _GroupSum(torch.autograd.Function):
    @staticmethod
    def forward(ctx, values, group, groups):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        output = values.new_zeros((groups, values.shape[1]))
        _RaggedAttention.library.group_sum_forward(
            values, group, output, values.shape[1], threads=values.numel(), group_size=256,
        )
        ctx.save_for_backward(group)
        ctx.shape = values.shape
        return output

    @staticmethod
    def backward(ctx, grad_output):
        group, = ctx.saved_tensors
        grad_input = grad_output.new_empty(ctx.shape)
        _RaggedAttention.library.group_sum_backward(
            grad_output.contiguous(), group, grad_input, ctx.shape[1],
            threads=grad_input.numel(), group_size=256,
        )
        return grad_input, None, None


class _IndexedGroupSum(torch.autograd.Function):
    @staticmethod
    def forward(ctx, values, source, group, groups):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        output = values.new_zeros((groups, values.shape[1]))
        _RaggedAttention.library.indexed_group_sum_forward(
            values, source, group, output, values.shape[1],
            threads=len(source) * values.shape[1], group_size=256,
        )
        ctx.save_for_backward(source, group)
        ctx.shape = values.shape
        return output

    @staticmethod
    def backward(ctx, grad_output):
        source, group = ctx.saved_tensors
        grad_input = grad_output.new_zeros(ctx.shape)
        _RaggedAttention.library.indexed_group_sum_backward(
            grad_output.contiguous(), source, group, grad_input, ctx.shape[1],
            threads=len(source) * ctx.shape[1], group_size=256,
        )
        return grad_input, None, None, None


class _ActionAttachments(torch.autograd.Function):
    @staticmethod
    def forward(ctx, nodes, path, target, *arguments):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        output = nodes.new_zeros((len(path), nodes.shape[1]))
        offset = 0
        for values, source in zip(*[iter(arguments)] * 2):
            _RaggedAttention.library.indexed_scatter_add(
                values, source, target, output, offset, nodes.shape[1],
                threads=len(source) * nodes.shape[1], group_size=256,
            )
            offset += len(source)
        _RaggedAttention.library.action_nodes_forward(
            nodes, path, output, nodes.shape[1], threads=output.numel(), group_size=256,
        )
        ctx.save_for_backward(path, target, *arguments[1::2])
        ctx.node_shape = nodes.shape
        ctx.value_shapes = [values.shape for values in arguments[::2]]
        return output

    @staticmethod
    def backward(ctx, grad_output):
        path, target, *sources = ctx.saved_tensors
        width = grad_output.shape[1]
        grad_nodes = grad_output.new_zeros(ctx.node_shape)
        _RaggedAttention.library.action_nodes_backward(
            grad_output.contiguous(), path, grad_nodes, width,
            threads=grad_output.numel(), group_size=256,
        )
        gradients, offset = [grad_nodes, None, None], 0
        for source, shape in zip(sources, ctx.value_shapes):
            grad_values = grad_output.new_zeros(shape)
            _RaggedAttention.library.indexed_scatter_backward(
                grad_output, source, target, grad_values, offset, width,
                threads=len(source) * width, group_size=256,
            )
            gradients.extend((grad_values, None))
            offset += len(source)
        return tuple(gradients)



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
        numeric = self.numeric(numeric)
        if (categorical.device.type == "mps" and numeric.dtype == torch.bfloat16
                and len(self.norm.weight) % 32 == 0 and len(self.norm.weight) <= 128):
            return _FusedSemantic.apply(
                categorical, numeric, embeddings, self.norm.weight, self.norm.bias,
            )
        return self.norm(nn.functional.embedding(categorical.long(), embeddings).sum(1) + numeric)


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
        self._sequence_layouts = []
        self._reuse_sequence_layouts = False
        self._sequence_layout_index = 0
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
        if (concepts.device.type == "mps" and torch.is_autocast_enabled("mps")
                and self.width == 128):
            arguments = tuple(value for domain, (name, *_), values in zip(
                self.encoders.values(), TOKEN_SPECS, domains,
            ) for value in (*values[:2], domain.numeric.weight,
                            domain.norm.weight, domain.norm.bias))
            return tuple(output[:values[2]] for output, values in zip(
                _FusedDomains.apply(concepts, *arguments), domains,
            ))
        return tuple(self.encoders[name](*values[:2], concepts)[:values[2]]
                     for (name, *_), values in zip(TOKEN_SPECS, domains))

    def _tag(self, values, role, collection=None):
        tagged = values + self.concepts.local("token_role", role).to(values.dtype)
        return tagged if collection is None else \
            tagged + self.concepts.local("collection", collection).to(values.dtype)

    @staticmethod
    def _domain_rows(domains, encoded, domain, predicate=None, scope_value=-1, source=None):
        _semantic, _numeric, _count, u, row, _scope, inverse, root, phase, _action = domains[domain]
        source = (root if scope_value == -1 else phase) if source is None else source
        if predicate is not None:
            source = source[predicate(u[source])]
        return encoded[domain][inverse[source]], row[source], u[source]

    def _sequence(self, values, group, groups, seed, transformer, selected=None):
        selected_count = selected
        flash = values.device.type == "mps" and self.width == 128 and self.heads == 4 \
            and torch.is_autocast_enabled("mps") \
            and torch.get_autocast_dtype("mps") == torch.bfloat16
        reuse = flash and getattr(self, "_reuse_sequence_layouts", False)
        if reuse:
            layout = self._sequence_layouts[self._sequence_layout_index]
            self._sequence_layout_index += 1
            order, inverse, selected, counts, lengths, offsets, destination, sequence, \
                block_rows, block_starts, metal_offsets, metal_lengths, sparse = layout
        else:
            order = torch.argsort(group, stable=True)
            inverse = torch.empty_like(order)
            inverse[order] = torch.arange(len(order), device=order.device)
            group = group[order]
            counts = torch.bincount(group, minlength=groups)
        if values.device.type == "mps" and self.width // self.heads in (16, 32) and not reuse:
            lengths = counts + 1
            padded_groups = (groups // 256 + 1) * 256
            lengths = nn.functional.pad(lengths, (0, padded_groups - groups), value=1)
            total = len(values) + padded_groups
            extra = (-total) % 65_536
            lengths[groups:] += extra // (padded_groups - groups)
            lengths[groups:groups + extra % (padded_groups - groups)] += 1
            offsets = torch.cat((counts.new_zeros(1), lengths.cumsum(0)))
            starts = torch.repeat_interleave(offsets[:groups], counts)
            position = torch.arange(len(values), device=values.device) \
                - torch.repeat_interleave(counts.cumsum(0) - counts, counts) + 1
            destination = (starts + position)[inverse]
            sequence = torch.repeat_interleave(
                torch.arange(len(lengths), device=values.device), lengths,
            )
            if flash:
                blocks = (lengths + 15) // 16
                block_rows = torch.repeat_interleave(
                    torch.arange(len(lengths), device=values.device), blocks,
                ).to(torch.int32)
                block_starts = torch.cat((counts.new_zeros(1), blocks.cumsum(0)[:-1])).to(
                    torch.int32
                )
                metal_offsets = offsets.to(torch.int32)
                metal_lengths = lengths.to(torch.int32)
            sparse = None
            if selected_count is not None:
                positions, query_order = torch.cat((
                    offsets[:-1], destination[-selected_count:],
                )).sort()
                real = (query_order >= len(lengths)) & \
                    (query_order < len(lengths) + selected_count)
                query_counts = torch.bincount(sequence[positions], minlength=len(lengths))
                query_offsets = torch.cat((query_counts.new_zeros(1), query_counts.cumsum(0)))
                sparse = positions, real, query_offsets, torch.argsort(query_order[real])
            if flash and self.training and torch.is_grad_enabled():
                self._sequence_layouts.append((
                    order, inverse, selected, counts, lengths, offsets, destination, sequence,
                    block_rows, block_starts, metal_offsets, metal_lengths, sparse,
                ))
        if values.device.type == "mps" and self.width // self.heads in (16, 32):
            total = len(sequence)
            current = values.new_zeros((total, self.width))
            current[offsets[:groups]] = seed.to(current.dtype)
            current[destination] = values.to(current.dtype)
            for layer_index, layer in enumerate(transformer.layers):
                if selected_count is not None and layer_index == len(transformer.layers) - 1:
                    positions, real, query_offsets, item_order = sparse
                    qkv = _norm_linear(
                        current, layer.norm1, layer.self_attn.in_proj_weight,
                        layer.self_attn.in_proj_bias,
                    ) if flash else nn.functional.linear(
                        layer.norm1(current), layer.self_attn.in_proj_weight,
                        layer.self_attn.in_proj_bias,
                    )
                    attended = _RaggedAttention.apply(
                        qkv, offsets.to(torch.int32), sequence.to(torch.int32),
                        layer.self_attn.num_heads, positions.to(torch.int32),
                        query_offsets.to(torch.int32), True,
                    )
                    current = _residual_linear(
                        attended, layer.self_attn.out_proj, current[positions],
                    )
                    current = _residual_linear(
                        layer.activation(_norm_linear(
                            current, layer.norm2, layer.linear1.weight, layer.linear1.bias,
                        )),
                        layer.linear2, current,
                    )
                    state = current[query_offsets[:groups]]
                    items = current[real][item_order]
                    return state, items
                qkv = _norm_linear(
                    current, layer.norm1, layer.self_attn.in_proj_weight,
                    layer.self_attn.in_proj_bias,
                ) if flash else nn.functional.linear(
                    layer.norm1(current), layer.self_attn.in_proj_weight,
                    layer.self_attn.in_proj_bias,
                )
                attended = _FlashAttention.apply(
                    qkv, metal_offsets, metal_lengths, block_rows, block_starts,
                ) if flash else _RaggedAttention.apply(
                    qkv, offsets.to(torch.int32), sequence.to(torch.int32),
                    layer.self_attn.num_heads, sequence.to(torch.int32), offsets.to(torch.int32),
                    False,
                )
                current = _residual_linear(attended, layer.self_attn.out_proj, current)
                current = _residual_linear(
                    layer.activation(_norm_linear(
                        current, layer.norm2, layer.linear1.weight, layer.linear1.bias,
                    )),
                    layer.linear2, current,
                )
            items = current[destination] if len(values) else values
            return current[offsets[:groups]], items
        values = values[order]
        selected = (torch.arange(len(values), device=values.device)
                    >= len(values) - selected_count)[order] if selected_count is not None else None
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
            summary = _GroupSum.apply(values, group, groups) \
                if values.device.type == "mps" and len(values) else \
                values.new_zeros((groups, self.width)).index_add_(
                    0, group, values.to(torch.float32),
                )
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

    def _actors(self, domains, encoded, batch, concepts, plan):
        order, player_source, player_group, move_raw, move_order, move_packed_order, \
            move_present, move_batch_sizes, effect_plans, families = plan
        actors, rows, u = self._domain_rows(domains, encoded, DOMAIN["actor"])
        if not len(actors):
            return actors, rows, u, [], []
        actors, rows, u = actors[order], rows[order], u[order]

        history, _history_rows, _history_u = self._domain_rows(
            domains, encoded, DOMAIN["history"],
        )
        if len(history):
            if len(player_source):
                actors = actors.index_add(
                    0, player_group, history[player_source].to(actors.dtype),
                )
            if len(move_raw):
                history_semantic, _numeric, _count, _u, _row, _scope, history_inverse, *_ = \
                    domains[DOMAIN["history"]]
                move_values = nn.functional.embedding(
                    history_semantic[history_inverse[move_raw], :1].long(), concepts,
                ).squeeze(1)[move_order]
                packed = nn.utils.rnn.PackedSequence(
                    move_values[move_packed_order], torch.tensor(move_batch_sizes),
                )
                _, output = self.move_gru(packed)
                move = move_values.new_zeros((len(actors), self.width)).index_copy(
                    0, move_present, output[0],
                )
                actors = actors + move

        effect_values = []
        for domain, source in zip(
            (DOMAIN["power"], DOMAIN["status"]), effect_plans,
        ):
            values, _effect_rows, _effect_u = self._domain_rows(domains, encoded, domain)
            effect_values.append(values[source])
        effects = torch.cat(effect_values) if effect_values else actors.new_empty((0, self.width))
        extra_values, extra_rows = [], []
        for name, (actor_index, effect_source, effect_actor, local_group) in zip(
            ("friendly_effect", "enemy_effect"), families,
        ):
            if not len(actor_index):
                continue
            values = effects[effect_source]
            mode = self.pooling[name]
            if mode == "global_tokens":
                extra_values.append(self._tag(values, 10, 8 + (name == "enemy_effect")))
                extra_rows.append(rows[effect_actor])
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
        action_row, action_flat, _legal, policy_sequence, actions, path, action_count, \
            attachment_rows, *_ = index
        action = self.action_encoder(semantic, numeric, concepts)[:action_count]
        if action.device.type == "mps":
            arguments = tuple(value for domain, row in zip(encoded, domains) if len(row[9])
                              for value in (domain, row[6][row[9]]))
            attached = _ActionAttachments.apply(
                nodes, path[:action_count], attachment_rows, *arguments,
            )
        else:
            attached = action.new_zeros(action.shape)
            values = [encoded[domain][row[6][row[9]]] for domain, row in enumerate(domains)
                      if len(row[9])]
            if values:
                attached.index_add_(0, attachment_rows, torch.cat(values).to(attached.dtype))
            present = path[:action_count] >= 0
            if present.any():
                attached[present] += nodes[path[:action_count][present]]
        action = self.action_norm(self._tag(action + attached, 13))
        return action, action_row[:action_count], action_flat[:action_count], actions, policy_sequence

    def _continuations(self, domains, encoded, plan):
        values, rows, u = self._domain_rows(domains, encoded, DOMAIN["continuation"])
        if not len(values):
            return values, rows, plan[3:]
        order, group, item_rows, groups, *_ = plan
        items = values.new_zeros((groups, self.width))
        items.index_add_(0, group, values[order])
        return items, item_rows, plan[3:]

    def encode_state(self, domains, encoded, index, action_values, action_index, concepts, batch):
        synchronize = torch.mps.synchronize if concepts.device.type == "mps" \
            and self.training and torch.is_grad_enabled() else lambda: None
        filters, actor_plan, continuation_plan = action_index[8:]
        current, nodes = self.encode_map(encoded, index)
        synchronize()
        action, action_rows, action_flat, actions, policy_sequence = self._actions(
            domains, encoded, action_values, action_index, nodes, concepts,
        )
        synchronize()
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
        synchronize()

        pool_specs = (
            ("card_pool", DOMAIN["card"], 0),
            ("relic_pool", DOMAIN["relic"], 1),
            ("encounter_pool", DOMAIN["encounter"], 2),
            ("event_pool", DOMAIN["event"], 3),
        )
        for name, domain, filtered in pool_specs:
            source = filters[filtered]
            if encoded[domain].device.type == "mps" and self.pooling["generation_pool"] == "sum":
                item_rows = domains[domain][4][source]
                summary = _IndexedGroupSum.apply(
                    encoded[domain], domains[domain][6][source], item_rows, batch,
                )
            else:
                item, item_rows, _ = self._domain_rows(
                    domains, encoded, domain, source=source,
                )
                summary = self._summarize(
                    item, item_rows, batch, name, self.pooling["generation_pool"],
                    transformer_name="generation_pool",
                )
            role = ("card_pool", "relic_pool", "encounter_pool", "event_pool").index(name) + 4
            add(self._tag(summary, role), torch.arange(batch, device=summary.device))
        synchronize()

        actors, actor_rows, actor_u, effect_values, effect_rows = self._actors(
            domains, encoded, batch, concepts, actor_plan,
        )
        synchronize()
        combat_rows = actor_rows[actor_u[:, 1] == 0]
        for name, domain, filtered, active in (
            ("deck", DOMAIN["card"], 4, torch.arange(batch, device=run.device)),
            ("hand", DOMAIN["card"], 5, combat_rows),
            ("draw", DOMAIN["card"], 6, combat_rows),
            ("discard", DOMAIN["card"], 7, combat_rows),
            ("exhaust", DOMAIN["card"], 8, combat_rows),
            ("relic", DOMAIN["relic"], 9, torch.arange(batch, device=run.device)),
            ("potion", DOMAIN["potion"], 10, torch.arange(batch, device=run.device)),
            ("orb", DOMAIN["orb"], None, combat_rows),
        ):
            item, item_rows, item_u = self._domain_rows(
                domains, encoded, domain,
                source=None if filtered is None else filters[filtered],
            )
            if name == "relic" and len(item):
                stored, stored_rows, _ = self._domain_rows(
                    domains, encoded, DOMAIN["card"], source=filters[11],
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
        synchronize()

        continuation, continuation_rows, continuation_gru = self._continuations(
            domains, encoded, continuation_plan,
        )
        mode = self.pooling["continuation"]
        if mode == "global_tokens":
            add(self._tag(continuation, 11, 10), continuation_rows)
        elif mode == "gru":
            _groups, maximum, position, present, last = continuation_gru
            padded = continuation.new_zeros((batch, maximum, self.width))
            if len(continuation):
                padded[continuation_rows, position] = continuation
                if maximum == 1 and continuation.device.type == "mps" \
                        and torch.is_autocast_enabled("mps"):
                    gates = _FastLinear.apply(
                        padded[:, 0], self.continuation_gru.weight_ih_l0,
                        self.continuation_gru.bias_ih_l0,
                    ).float()
                    reset, update, candidate = gates.chunk(3, 1)
                    hidden_reset, hidden_update, hidden_candidate = \
                        self.continuation_gru.bias_hh_l0.chunk(3)
                    reset = torch.sigmoid(reset + hidden_reset)
                    update = torch.sigmoid(update + hidden_update)
                    output = (1 - update) * torch.tanh(
                        candidate + reset * hidden_candidate
                        + self.continuation_gru.weight_hh_l0[0, 0] * 0
                    )
                    summary = continuation.new_zeros((batch, self.width)).index_copy(
                        0, present, output[present],
                    )
                else:
                    output, _ = self.continuation_gru(padded)
                    summary = continuation.new_zeros((batch, self.width)).index_copy(
                        0, present, output[present, last],
                    )
            else:
                summary = continuation.new_zeros((batch, self.width))
            add(self._tag(summary, 14, 10),
                torch.arange(batch, device=run.device))
        else:
            add(self._tag(self._summarize(
                    continuation, continuation_rows, batch, "continuation", mode), 14, 10),
                torch.arange(batch, device=run.device))
        crystal, crystal_rows, _ = self._domain_rows(domains, encoded, DOMAIN["crystal"])
        add(self._tag(crystal, 12), crystal_rows)
        add(action, action_rows)
        synchronize()

        values = torch.cat(values)
        rows = torch.cat(rows)
        self._sequence_lengths = torch.bincount(rows, minlength=batch) + 1
        state, transformed = self._sequence(
            values, rows, batch, self.concepts.local("token_role", 0), self.global_transformer,
            len(action),
        )
        synchronize()
        return state, transformed if len(action) else action, \
            action_rows, action_flat, actions, policy_sequence

    def forward(self, character, _globals, domains, state_index, action_values, action_index,
                return_state=False, policy_only=False, flat_policy=False, temperature=1):
        synchronize = torch.mps.synchronize if domains[0][0].device.type == "mps" \
            and self.training and torch.is_grad_enabled() else lambda: None
        synchronize()
        if self.training and torch.is_grad_enabled():
            self._sequence_layouts.clear()
        if self._reuse_sequence_layouts:
            self._sequence_layout_index = 0
        concepts = self.concepts.flattened()
        encoded = self.encode_domains(domains, concepts)
        synchronize()
        state, action, action_row, action_flat, actions, sequence = self.encode_state(
            domains, encoded, state_index, action_values, action_index, concepts, len(character),
        )
        synchronize()
        scores = _norm_head(action, self.global_norm, self.policy) / temperature
        legal = torch.ones_like(scores, dtype=torch.bool)
        scores = self.group_log_softmax(scores, legal, sequence)
        synchronize()
        if flat_policy:
            policy = scores
        else:
            policy = scores.new_zeros(len(state) * actions).scatter(
                0, action_flat, scores,
            ).reshape(len(state), actions)
        if policy_only:
            return policy
        output = policy, _norm_head(state, self.global_norm, self.critic)[:, None]
        synchronize()
        return (*output, self.global_norm(state)) if return_state else output


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
    prediction = predictions.float().detach().cpu().numpy() \
        if torch.is_tensor(predictions) else np.asarray(predictions, np.float32)
    target = targets.float().detach().cpu().numpy() \
        if torch.is_tensor(targets) else np.asarray(targets, np.float32)
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
    unique_rows = [domain[6:] if len(domain) == 8 else None for domain in domains]
    domains = [domain[:6] for domain in domains]
    pad = model.training and model.global_norm.weight.device.type == "mps"
    bucket = lambda count: 0 if not count else min(
        size for power in range(max(1, (count - 1).bit_length()), 64)
        for size in (3 * (1 << power) // 4, 1 << power) if size >= count
    )
    tensor = lambda value: torch.as_tensor(value, dtype=torch.long, device=target)
    domain_tensors = []
    inverse_rows = []
    for (name, *_), (u, _s, semantic, numeric, row, scope), unique in zip(
        TOKEN_SPECS, domains, unique_rows,
    ):
        if semantic.max(initial=0) >= model.concepts.num_embeddings:
            raise ValueError(f"invalid {name} semantic id")
        if unique is not None:
            first, inverse = unique
        elif len(semantic):
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
            *(torch.as_tensor(np.flatnonzero(predicate), device=target)
              for predicate in (scope == -1, scope == -2, scope >= 0)),
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
    def filtered(domain, predicate):
        u, *_values, scope = domains[domain]
        return tensor(np.flatnonzero((scope == -1) & predicate(u[:, 0])))
    filters = (
        filtered(DOMAIN["card"], lambda value: value == 13),
        filtered(DOMAIN["relic"], lambda value: (value == 1) | (value == 2)),
        filtered(DOMAIN["encounter"], lambda value: value <= 2),
        filtered(DOMAIN["event"], lambda value: value == 0),
        *(filtered(DOMAIN["card"], lambda value, kind=kind: value == kind)
          for kind in range(5)),
        filtered(DOMAIN["relic"], lambda value: value == 0),
        filtered(DOMAIN["potion"], lambda value: value == 0),
        filtered(DOMAIN["card"], lambda value: value == 8),
    )
    actor_u, _s, _c, _f, actor_row, actor_scope = domains[DOMAIN["actor"]]
    actor_source = np.flatnonzero(actor_scope == -1)
    actor_order = np.argsort(
        actor_row[actor_source].astype(np.int64) * (1 << 20) + actor_u[actor_source, 0],
        kind="stable",
    )
    actor_source = actor_source[actor_order]
    actor_keys = actor_row[actor_source].astype(np.int64) * (1 << 20) + actor_u[actor_source, 0]
    history_u, _s, _c, _f, history_row, history_scope = domains[DOMAIN["history"]]
    history_source = np.flatnonzero(history_scope == -1)
    history_keys = history_row[history_source].astype(np.int64) * (1 << 20) \
        + history_u[history_source, 0]
    history_group = np.searchsorted(actor_keys, history_keys)
    history_valid = history_group < len(actor_keys)
    history_valid &= actor_keys[np.minimum(history_group, max(0, len(actor_keys) - 1))] \
        == history_keys if len(actor_keys) else False
    player_source = np.flatnonzero(history_valid & (history_u[history_source, 1] == 0))
    move_source = np.flatnonzero(history_valid & (history_u[history_source, 1] == 1))
    move_order = np.argsort(history_group[move_source], kind="stable")
    move_group = history_group[move_source][move_order]
    move_counts = np.bincount(move_group, minlength=len(actor_source))
    move_present = np.flatnonzero(move_counts)
    compact_move_group = np.repeat(np.arange(len(move_present)), move_counts[move_present])
    move_position = np.arange(len(compact_move_group)) \
        - np.repeat(np.cumsum(move_counts) - move_counts, move_counts)
    length_order = np.argsort(-move_counts[move_present], kind="stable")
    length_inverse = np.empty_like(length_order)
    length_inverse[length_order] = np.arange(len(length_order))
    compact_move_group = length_inverse[compact_move_group]
    move_present = move_present[length_order]
    move_packed_order = np.lexsort((compact_move_group, move_position))
    move_batch_sizes = tuple(int((move_counts[move_present] > index).sum())
                             for index in range(move_counts.max(initial=0)))
    effect_plans = []
    effect_groups = []
    for domain in (DOMAIN["power"], DOMAIN["status"]):
        effect_u, _s, _c, _f, effect_row, effect_scope = domains[domain]
        effect_source = np.flatnonzero(effect_scope == -1)
        effect_keys = effect_row[effect_source].astype(np.int64) * (1 << 20) \
            + effect_u[effect_source, 0]
        group = np.searchsorted(actor_keys, effect_keys)
        valid = group < len(actor_keys)
        valid &= actor_keys[np.minimum(group, max(0, len(actor_keys) - 1))] == effect_keys \
            if len(actor_keys) else False
        source = np.flatnonzero(valid)
        effect_plans.append(tensor(source))
        effect_groups.append(group[source])
    effect_groups = np.concatenate(effect_groups)
    families = []
    for family in (actor_u[actor_source, 1] < 2, actor_u[actor_source, 1] == 2):
        actor_index = np.flatnonzero(family)
        lookup = np.full(len(actor_source), -1, np.int64)
        lookup[actor_index] = np.arange(len(actor_index))
        selected = lookup[effect_groups] >= 0
        effect_source = np.flatnonzero(selected)
        effect_actor = effect_groups[selected]
        families.append((
            tensor(actor_index), tensor(effect_source), tensor(effect_actor),
            tensor(lookup[effect_actor]),
        ))
    actor_plan = (
        tensor(actor_order), tensor(player_source), tensor(history_group[player_source]),
        tensor(history_source[move_source]), tensor(move_order), tensor(move_packed_order),
        tensor(move_present), move_batch_sizes, tuple(effect_plans), tuple(families),
    )
    continuation_u, _s, _c, _f, continuation_row, continuation_scope = \
        domains[DOMAIN["continuation"]]
    continuation_source = np.flatnonzero(continuation_scope == -1)
    continuation_order = np.argsort(
        continuation_row[continuation_source].astype(np.int64) * (1 << 32)
        + continuation_u[continuation_source, 8], kind="stable",
    )
    ordered_rows = continuation_row[continuation_source][continuation_order]
    ordered_u = continuation_u[continuation_source][continuation_order]
    keys = ordered_rows.astype(np.int64) * (1 << 32) + ordered_u[:, 2]
    first = np.r_[True, keys[1:] != keys[:-1]] if len(keys) else np.empty(0, bool)
    group = np.cumsum(first) - 1
    item_rows = ordered_rows[first]
    counts = np.bincount(item_rows, minlength=batch)
    present = np.flatnonzero(counts)
    position = np.arange(len(item_rows)) - np.repeat(np.cumsum(counts) - counts, counts)
    continuation_plan = (
        tensor(continuation_order), tensor(group), tensor(item_rows), int(first.sum()),
        int(counts.max(initial=0)), tensor(position), tensor(present),
        tensor(counts[present] - 1),
    )
    action_index = (
        tensor(action_row), tensor(action_flat), torch.as_tensor(action_legal, device=target),
        candidate_index(action_row, batch, target, True), actions, tensor(path), action_count,
        tensor(np.concatenate([scope[scope >= 0] for *_values, scope in domains])),
        filters, actor_plan, continuation_plan,
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
