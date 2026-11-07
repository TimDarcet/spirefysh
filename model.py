import copy
import contextvars
import fcntl
import functools
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

FEATURE_VERSION, MODEL_VERSION, CATEGORIES, MAX_PROGRESS, DOMAINS, PLACES, SEMANTIC_NAMES, \
    POSITION_CAPS = sts2_sim.model_schema()
DOMAINS = tuple(tuple(row) for row in DOMAINS)
DOMAIN = {name: index for index, (name, *_shape) in enumerate(DOMAINS)}
PLACES = tuple(PLACES)
PLACE = {name: index for index, name in enumerate(PLACES)}
SEMANTIC_NAMES = tuple(SEMANTIC_NAMES)
POSITION_CAPS = dict(POSITION_CAPS)
RAW_DOMAINS = ("potion", "move", "map_edge")
COLLECTIONS = ("deck", "hand", "draw", "discard", "exhaust", "relic", "potion", "orb")
DEFAULT_ARCHITECTURE = (128, 4, 8, 384)
PRECISIONS = ("fp32", "bf16")
TRAINING_PRECISIONS = (*PRECISIONS, "bf16_with_fp32_dV")
WINNING_CAPACITY = 0
CHANGE = "Unified tokens (concept codes with per-concept amount embeddings, parent/link structure); simulator action previews and intent damage; ln(1+HP); energy ÷5; powers and statuses unified."
POOLS = ("removed_relic", "encounter_pool", "event_pool")
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
    "continuation": "gru",
}
STAGES = [(0, bonus) for bonus in (24, 20, 16, 12, 8, 4, 3, 2, 1, 0)] + [
    (ascension, 0) for ascension in range(1, 11)
]

_GRAPH_RECORDER = contextvars.ContextVar("model_graph_recorder", default=None)


def graph_name(value, name):
    if (recorder := _GRAPH_RECORDER.get()) is not None:
        recorder.name(value, name)
    return value


POTENTIAL_TERMS = (
    "health", "max_hp", "upgrades", "relics", "potions", "gold", "cards",
    "uncommon_relics", "rare_relics", "uncommon_potions", "rare_potions",
    "uncommon_cards", "rare_cards",
)
RUN_FEATURES = 2 * len(POTENTIAL_TERMS)


def potential_head(weights, mode):
    health, *resources = weights
    if len(resources) + 1 != len(POTENTIAL_TERMS) or not all(map(math.isfinite, weights)):
        raise ValueError("invalid potential weights")
    if mode not in ("health_multiplier", "additive"):
        raise ValueError("invalid potential mode")
    zeros = [0.] * len(resources)
    return [1., health] + (zeros + resources if mode == "health_multiplier" else resources + zeros)


OPTIMIZER_CONFIG = {"type": "Adam", "betas": (.9, .99), "eps": 1e-5}

_ACCELERATOR_LOCK = None
_TRAINING_METAL = r"""
#include <metal_stdlib>
using namespace metal;
constant uint SIMD_WIDTH = 32;
constant uint TOKEN_REDUCTION = 128;

kernel void gru_step_forward(
    device const bfloat *input, device const bfloat *hidden, device const float *previous,
    device float *output, constant uint& width, uint index [[thread_position_in_grid]]) {
    uint row = index / width, column = index % width, offset = row * 3 * width + column;
    float reset = 1.0f / (1.0f + exp(-float(input[offset]) - float(hidden[offset])));
    float update = 1.0f / (1.0f + exp(-float(input[offset + width]) - float(hidden[offset + width])));
    float candidate = tanh(float(input[offset + 2 * width])
        + reset * float(hidden[offset + 2 * width]));
    output[index] = (1.0f - update) * candidate + update * previous[index];
}

kernel void gru_step_backward(
    device const bfloat *input, device const bfloat *hidden, device const float *previous,
    device const float *grad_output, device bfloat *grad_input, device bfloat *grad_hidden,
    device float *grad_previous, constant uint& width, uint index [[thread_position_in_grid]]) {
    uint row = index / width, column = index % width, offset = row * 3 * width + column;
    float hidden_candidate = float(hidden[offset + 2 * width]);
    float reset = 1.0f / (1.0f + exp(-float(input[offset]) - float(hidden[offset])));
    float update = 1.0f / (1.0f + exp(-float(input[offset + width]) - float(hidden[offset + width])));
    float candidate = tanh(float(input[offset + 2 * width]) + reset * hidden_candidate);
    float grad = grad_output[index];
    float candidate_grad = grad * (1.0f - update) * (1.0f - candidate * candidate);
    float reset_grad = candidate_grad * hidden_candidate * reset * (1.0f - reset);
    float update_grad = grad * (previous[index] - candidate) * update * (1.0f - update);
    grad_input[offset] = bfloat(reset_grad);
    grad_input[offset + width] = bfloat(update_grad);
    grad_input[offset + 2 * width] = bfloat(candidate_grad);
    grad_hidden[offset] = bfloat(reset_grad);
    grad_hidden[offset + width] = bfloat(update_grad);
    grad_hidden[offset + 2 * width] = bfloat(candidate_grad * reset);
    grad_previous[index] = grad * update;
}

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

kernel void attention_backward_value_bfloat(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device const int *selected, device const int *query_offsets,
    device const float *lse, device const bfloat *grad_output, device float *grad_value,
    constant uint& heads, constant uint& dimension, constant bool& sparse,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, key = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[key], begin = sparse ? query_offsets[row] : offsets[row];
    int end = sparse ? query_offsets[row + 1] : offsets[row + 1];
    uint width = heads * dimension, k = key * 3 * width + width + head * dimension;
    float key_value = float(qkv[k + column]), dv = 0.0f;
    for (int query_index = begin; query_index < end; ++query_index) {
        uint token = sparse ? selected[query_index] : query_index;
        uint query = token * 3 * width + head * dimension + column;
        uint out = query_index * width + head * dimension + column;
        float probability = exp(head_sum(float(qkv[query]) * key_value, dimension, lane)
            * rsqrt(float(dimension)) - lse[query_index * heads + head]);
        dv += probability * float(grad_output[out]);
    }
    grad_value[key * width + head * dimension + column] = dv;
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

template <typename G>
void layer_norm_backward(
    device const float *input, device const float *gamma, device const float *stats,
    device const G *grad_output, device const float *residual, device float *grad_input,
    device float *partial, uint rows, uint block_rows, uint width, bool add_residual, uint block,
    uint lane) {
    float4 grad_gamma = 0.0f, grad_beta = 0.0f;
    for (uint row = block * block_rows; row < min(rows, (block + 1) * block_rows); ++row) {
        float mean = stats[row * 2], inverse_std = stats[row * 2 + 1];
        float4 normalized, grad;
        float local_sum = 0.0f, local_product = 0.0f;
        for (uint part = 0; part < width / 32; ++part) {
            uint column = lane + part * 32, index = row * width + column;
            float gradient = float(grad_output[index]);
            normalized[part] = (input[index] - mean) * inverse_std;
            grad_gamma[part] += gradient * normalized[part];
            grad_beta[part] += gradient;
            grad[part] = gradient * gamma[column];
            local_sum += grad[part];
            local_product += grad[part] * normalized[part];
        }
        float sum = simd_sum(local_sum), product = simd_sum(local_product);
        for (uint part = 0; part < width / 32; ++part) {
            uint index = row * width + lane + part * 32;
            grad_input[index] = (
                grad[part] - sum / width - normalized[part] * product / width
            ) * inverse_std + (add_residual ? residual[index] : 0.0f);
        }
    }
    for (uint part = 0; part < width / 32; ++part) {
        partial[block * 2 * width + lane + part * 32] = grad_gamma[part];
        partial[(block * 2 + 1) * width + lane + part * 32] = grad_beta[part];
    }
}

#define LAYER_NORM_BACKWARD(name, G) \
kernel void name( \
    device const float *input, device const float *gamma, device const float *stats, \
    device const G *grad_output, device const float *residual, device float *grad_input, \
    device float *partial, constant uint& rows, constant uint& block_rows, constant uint& width, \
    constant uint& add_residual, uint block [[threadgroup_position_in_grid]], \
    uint lane [[thread_index_in_simdgroup]]) { \
    layer_norm_backward(input, gamma, stats, grad_output, residual, grad_input, partial, rows, \
                        block_rows, width, add_residual, block, lane); \
}
LAYER_NORM_BACKWARD(layer_norm_backward_bfloat, bfloat)
LAYER_NORM_BACKWARD(layer_norm_backward_float, float)

kernel void layer_norm_forward_float(
    device const float *input, device const float *gamma, device const float *beta,
    device float *output, device float *stats, constant uint& width,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint row = tid / 32, base = row * width + lane;
    float4 value;
    float local_sum = 0.0f, local_square = 0.0f;
    for (uint part = 0; part < width / 32; ++part) {
        value[part] = input[base + part * 32];
        local_sum += value[part];
    }
    float mean = simd_sum(local_sum) / width;
    for (uint part = 0; part < width / 32; ++part)
        local_square += (value[part] - mean) * (value[part] - mean);
    float inverse_std = rsqrt(simd_sum(local_square) / width + 1e-5f);
    for (uint part = 0; part < width / 32; ++part) {
        uint column = lane + part * 32;
        output[base + part * 32] = (value[part] - mean) * inverse_std * gamma[column] + beta[column];
    }
    if (lane == 0) {
        stats[row * 2] = mean;
        stats[row * 2 + 1] = inverse_std;
    }
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


def _committed(function):
    # Commit the MPS command buffer after each custom op so the GPU runs while Python encodes ahead.
    for name in ("forward", "backward"):
        def committed(ctx, *args, method=getattr(function, name)):
            output = method(ctx, *args)
            first = next(value for value in (output if isinstance(output, tuple) else (output,))
                         if value is not None)
            if first.numel() >= 1 << 22:
                _committed.event = getattr(_committed, "event", None) or torch.mps.Event()
                _committed.event.record()
            return output
        setattr(function, name, staticmethod(committed))
    return function


@_committed
class _RaggedAttention(torch.autograd.Function):
    library = None
    half_library = None

    @staticmethod
    def forward(ctx, qkv, offsets, sequence, heads, selected, query_offsets, sparse,
                fp32_value_backward=False):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        if qkv.dtype == torch.float16 and _RaggedAttention.half_library is None:
            _RaggedAttention.half_library = torch.mps.compile_shader(
                _TRAINING_METAL.replace("bfloat", "half")
            )
        library = _RaggedAttention.half_library \
            if qkv.dtype == torch.float16 else _RaggedAttention.library
        queries = len(selected) if sparse else len(qkv)
        output = qkv.new_empty((queries, qkv.shape[1] // 3))
        lse = torch.empty((queries, heads), dtype=torch.float32, device=qkv.device)
        dimension = output.shape[1] // heads
        suffix = "_half" if qkv.dtype == torch.float16 else \
            "_bfloat" if qkv.dtype == torch.bfloat16 else ""
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
        ctx.fp32_value_backward = fp32_value_backward
        ctx.library, ctx.suffix = library, suffix
        ctx.sparse = sparse
        return output

    @staticmethod
    def backward(ctx, grad_output):
        qkv, offsets, sequence, selected, query_offsets, output, lse = ctx.saved_tensors
        grad_output = grad_output.contiguous()
        grad_qkv = torch.empty_like(qkv)
        delta = torch.empty_like(lse)
        tiled = bool(ctx.suffix) and ctx.dimension == 32
        query_threads = ((len(output) * ctx.heads + 3) // 4 * 32 if tiled
                         else len(output) * ctx.heads * ctx.dimension)
        args = qkv, offsets, sequence, selected, output, lse, grad_output
        query = getattr(
            ctx.library, f"attention_backward_query{ctx.suffix}{'_x4' if tiled else ''}",
        ) if ctx.suffix else ctx.library.backward_query
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
        key_threads = ((len(qkv) * ctx.heads + 3) // 4 * 32 if tiled
                       else len(qkv) * ctx.heads * ctx.dimension)
        arguments = (
            qkv, offsets, sequence, selected, query_offsets, output, lse,
            grad_output, delta, grad_qkv,
        )
        key_value = getattr(
            ctx.library,
            f"attention_backward_key_value{ctx.suffix}{'_x4' if tiled else ''}",
        ) if ctx.suffix else ctx.library.backward_key_value
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
        # The tiled kernels already accumulate dV in fp32 from fp32 probabilities.
        if ctx.fp32_value_backward and qkv.dtype == torch.bfloat16 and not tiled:
            width = qkv.shape[1] // 3
            grad_value = torch.empty((len(qkv), width), dtype=torch.float32, device=qkv.device)
            _RaggedAttention.library.attention_backward_value_bfloat(
                qkv, offsets, sequence, selected, query_offsets, lse, grad_output,
                grad_value, ctx.heads, ctx.dimension, ctx.sparse,
                threads=len(qkv) * ctx.heads * ctx.dimension, group_size=32,
            )
            grad_qkv = torch.cat((grad_qkv[:, :2 * width], grad_value.to(qkv.dtype)), 1)
        return grad_qkv, None, None, None, None, None, None, None


@_committed
class _SparseAttention(torch.autograd.Function):
    # Final-layer attention of sparse queries (rows of q in sequence order) over their sequences' keys
    # and values (rows of kv), 4 heads of width 32.
    @staticmethod
    def forward(ctx, q, kv, offsets, query_offsets):
        library = _FlashAttention.compiled()
        output = q.new_empty(q.shape)
        lse = torch.empty((len(q), 4), dtype=torch.float32, device=q.device)
        library.sparse_forward(offsets, query_offsets, q, kv, output, lse,
                               threads=(len(offsets) - 1) * 128, group_size=128)
        ctx.save_for_backward(q, kv, offsets, query_offsets, output, lse)
        return output

    @staticmethod
    def backward(ctx, grad_output):
        q, kv, offsets, query_offsets, output, lse = ctx.saved_tensors
        grad_output = grad_output.to(torch.bfloat16).contiguous()
        grad_q, grad_kv, delta = torch.empty_like(q), torch.empty_like(kv), torch.empty_like(lse)
        arguments, threads, library = (offsets, query_offsets, q, kv), (len(offsets) - 1) * 128, _FlashAttention.library
        library.sparse_delta(output, grad_output, delta, threads=delta.numel(), group_size=256)
        library.sparse_backward_key_value(*arguments, lse, delta, grad_output, grad_kv,
                                          threads=threads, group_size=128)
        library.sparse_backward_query(*arguments, output, lse, grad_output, grad_q,
                                      threads=threads, group_size=128)
        return grad_q, grad_kv, None, None


@_committed
class _FlashAttention(torch.autograd.Function):
    library = None

    @staticmethod
    def compiled():
        if _FlashAttention.library is None:
            _FlashAttention.library = torch.mps.compile_shader(
                Path(__file__).with_name("attention.metal").read_text()
            )
        return _FlashAttention.library

    @staticmethod
    def forward(ctx, qkv, offsets, lengths, chunk_rows, chunk_starts, real_tokens):
        _FlashAttention.compiled()
        output = qkv.new_empty((len(qkv), qkv.shape[1] // 3))
        output[real_tokens:] = 0
        lse = torch.empty((len(qkv), 4), dtype=torch.float32, device=qkv.device)
        layout = offsets, lengths, chunk_rows, chunk_starts
        threads = len(chunk_rows) * 4 * 128
        _FlashAttention.library.flash_forward(
            *layout, qkv, output, lse, threads=threads, group_size=128,
        )
        ctx.save_for_backward(qkv, output, lse, *layout)
        ctx.threads, ctx.real_tokens = threads, real_tokens
        return output

    @staticmethod
    def backward(ctx, grad_output):
        qkv, output, lse, *layout = ctx.saved_tensors
        grad_output = grad_output.contiguous()
        grad_qkv = torch.empty_like(qkv)
        grad_qkv[ctx.real_tokens:] = 0
        delta = torch.empty_like(lse)
        _FlashAttention.library.flash_backward_query(
            *layout, qkv, output, lse, grad_output, grad_qkv, delta,
            threads=ctx.threads, group_size=128,
        )
        _FlashAttention.library.flash_backward_key_value(
            *layout, qkv, lse, delta, grad_output, grad_qkv,
            threads=ctx.threads, group_size=128,
        )
        return grad_qkv, None, None, None, None, None


@_committed
class _GruStep(torch.autograd.Function):
    @staticmethod
    def forward(ctx, input_gates, hidden_gates, previous):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        output = torch.empty_like(previous)
        _RaggedAttention.library.gru_step_forward(
            input_gates, hidden_gates, previous, output, previous.shape[1],
            threads=previous.numel(), group_size=128,
        )
        ctx.save_for_backward(input_gates, hidden_gates, previous)
        return output

    @staticmethod
    def backward(ctx, grad_output):
        input_gates, hidden_gates, previous = ctx.saved_tensors
        grad_input = torch.empty_like(input_gates)
        grad_hidden = torch.empty_like(hidden_gates)
        grad_previous = torch.empty_like(previous)
        _RaggedAttention.library.gru_step_backward(
            input_gates, hidden_gates, previous, grad_output.contiguous(),
            grad_input, grad_hidden, grad_previous, previous.shape[1],
            threads=previous.numel(), group_size=128,
        )
        return grad_input, grad_hidden, grad_previous


def _linear_library():
    if _FastLinear.library is None:
        _FastLinear.library = torch.mps.compile_shader(
            Path(__file__).with_name("linear.metal").read_text()
        )
    return _FastLinear.library


def _linear_kernel(name, a, b, output, rows, columns, depth, bias=None, residual=None, splits=1):
    getattr(_linear_library(), name)(
        a, b, output, output if bias is None else bias, output if residual is None else residual,
        rows, columns, depth, (depth + splits - 1) // splits,
        threads=((columns + 31) // 32 * 32, (rows + 31) // 32, splits), group_size=(32, 1, 1),
    )


def _linear_forward(value_bfloat, weight, bias, residual=None, hidden=None):
    # With hidden, the biased output goes to hidden and its gelu is returned.
    weight_bfloat = weight.to(torch.bfloat16)
    output = value_bfloat.new_empty(
        (len(value_bfloat), len(weight)),
        dtype=torch.float32 if residual is not None else torch.bfloat16,
    )
    bias = bias.float() if bias is not None else weight.new_zeros(len(weight), dtype=torch.float32)
    _linear_kernel(
        "linear_gelu" if hidden is not None else "linear" if residual is None else "linear_residual",
        value_bfloat, weight_bfloat, output, len(value_bfloat), len(weight), value_bfloat.shape[1], bias,
        residual if hidden is None else hidden,
    )
    return output, weight_bfloat


def _linear_residual_norm(value_bfloat, weight, bias, residual, gamma, beta):
    # residual + linear(value) of width 128, with the following layer norm's bf16 output and stats.
    weight_bfloat, rows = weight.to(torch.bfloat16), len(value_bfloat)
    output, stats = torch.empty_like(residual), residual.new_empty((rows, 2))
    normalized = torch.empty_like(residual, dtype=torch.bfloat16)
    _linear_library().linear_residual_norm(
        value_bfloat, weight_bfloat, output, bias.float(), residual.contiguous(), gamma, beta, normalized,
        stats, rows, value_bfloat.shape[1], threads=(rows + 31) // 32 * 128, group_size=128,
    )
    return output, weight_bfloat, normalized, stats


def _linear_weight_backward(value, grad_output):
    grad_weight = value.new_zeros((grad_output.shape[1], value.shape[1]), dtype=torch.float32)
    grad_bias = value.new_zeros(grad_output.shape[1], dtype=torch.float32)
    _linear_kernel(
        "linear_weight_grad" + ("_float" if grad_output.dtype == torch.float32 else ""), grad_output,
        value, grad_weight, grad_output.shape[1], value.shape[1], len(value), grad_bias,
        splits=max(1, min(128, len(value) // 128)),
    )
    return grad_weight, grad_bias


def _linear_backward(value, weight, grad_output, hidden=None, norm=None, extra=None):
    # With hidden, value is gelu(hidden) and grad_value is the gradient of hidden. With
    # norm = (input, gamma, stats, residual), value is layer_norm(input) of width 128 and grad_value is
    # (gradient of input plus residual, gradient of gamma, gradient of beta); extra = (rows, inverse)
    # then adds rows[inverse[m]] to the gradient of normalized row m where inverse[m] >= 0.
    grad_output = grad_output.contiguous() if grad_output.dtype == torch.float32 \
        else grad_output.to(torch.bfloat16).contiguous()
    suffix = "_float" if grad_output.dtype == torch.float32 else ""
    gelu = "_gelu" if hidden is not None else ""
    rows, columns, depth = len(value), value.shape[1], len(weight)
    if norm is None:
        grad_value = torch.empty_like(value)
        _linear_kernel(
            "linear_input_grad" + suffix + gelu, grad_output, weight, grad_value, rows, columns, depth,
            residual=hidden,
        )
    else:
        input, gamma, stats, residual = norm
        blocks = (rows + 31) // 32
        grad_input, partial = torch.empty_like(input), input.new_empty((blocks, 2, columns))
        getattr(_linear_library(), "linear_input_grad_norm" + suffix)(
            grad_output, weight, input, gamma, stats, input if residual is None else residual.contiguous(),
            grad_input, partial, rows, depth, residual is not None, *(extra or (input, stats)),
            extra is not None, threads=blocks * 128, group_size=128,
        )
        grad_value = grad_input, *_norm_reduce(partial, gamma)
    return grad_value, *_linear_weight_backward(value, grad_output)


@_committed
class _FastLinear(torch.autograd.Function):
    library = None

    @staticmethod
    def forward(ctx, value, weight, bias):
        value_bfloat = value.to(torch.bfloat16)
        output, weight_bfloat = _linear_forward(value_bfloat, weight, bias)
        ctx.save_for_backward(value_bfloat, weight_bfloat)
        ctx.value_dtype, ctx.bias = value.dtype, bias is not None
        return output

    @staticmethod
    def backward(ctx, grad_output):
        value, weight = ctx.saved_tensors
        grad_value, grad_weight, grad_bias = _linear_backward(value, weight, grad_output)
        return grad_value.to(ctx.value_dtype), grad_weight, grad_bias if ctx.bias else None


@_committed
class _LinearResidual(torch.autograd.Function):
    # With gamma and beta, also returns the following layer norm's bf16 output and stats.
    @staticmethod
    def forward(ctx, value, weight, bias, residual, gamma=None, beta=None):
        value_bfloat = value.to(torch.bfloat16)
        if gamma is None:
            output, weight_bfloat = _linear_forward(value_bfloat, weight, bias, residual)
        else:
            output, weight_bfloat, normalized, stats = _linear_residual_norm(
                value_bfloat, weight, bias, residual, gamma, beta,
            )
            ctx.mark_non_differentiable(normalized, stats)
        ctx.save_for_backward(value_bfloat, weight_bfloat)
        ctx.value_dtype, ctx.bias = value.dtype, bias is not None
        return output if gamma is None else (output, normalized, stats)

    @staticmethod
    def backward(ctx, grad_output, *_):
        value, weight = ctx.saved_tensors
        grad_value, grad_weight, grad_bias = _linear_backward(value, weight, grad_output)
        return grad_value.to(ctx.value_dtype), grad_weight, grad_bias if ctx.bias else None, \
            grad_output, None, None


def _norm_reduce(partial, gamma):
    grad_gamma, grad_beta = torch.empty_like(gamma), torch.empty_like(gamma)
    _RaggedAttention.library.token_backward_norm_reduce(
        partial, grad_gamma, grad_beta, len(partial), len(gamma), threads=len(gamma) * 128, group_size=128,
    )
    return grad_gamma, grad_beta


def _layer_norm_backward(value, gamma, stats, grad_normalized, residual=None, block_rows=32):
    rows, width = value.shape
    blocks = (rows + block_rows - 1) // block_rows
    grad_value, partial = torch.empty_like(value), value.new_empty((blocks, 2, width))
    getattr(_RaggedAttention.library, "layer_norm_backward_" + (
        "float" if grad_normalized.dtype == torch.float32 else "bfloat"
    ))(
        value, gamma, stats, grad_normalized, value if residual is None else residual.contiguous(),
        grad_value, partial, rows, block_rows, width, residual is not None,
        threads=blocks * 32, group_size=32,
    )
    return grad_value, *_norm_reduce(partial, gamma)


@_committed
class _LayerNorm(torch.autograd.Function):
    @staticmethod
    def forward(ctx, value, gamma, beta):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        output, stats = torch.empty_like(value), value.new_empty((len(value), 2))
        _RaggedAttention.library.layer_norm_forward_float(
            value, gamma, beta, output, stats, value.shape[1],
            threads=len(value) * 32, group_size=32,
        )
        ctx.save_for_backward(value, gamma, stats)
        return output

    @staticmethod
    def backward(ctx, grad_output):
        value, gamma, stats = ctx.saved_tensors
        return _layer_norm_backward(value, gamma, stats, grad_output.float().contiguous())


def _layer_norm_forward(value, gamma, beta):
    if _RaggedAttention.library is None:
        _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
    normalized = torch.empty_like(value, dtype=torch.bfloat16)
    stats = value.new_empty((len(value), 2))
    _RaggedAttention.library.layer_norm_forward_bfloat(
        value, gamma, beta, normalized, stats, len(value), value.shape[1],
        threads=len(value) * 32, group_size=32,
    )
    return normalized, stats


@_committed
class _NormLinear(torch.autograd.Function):
    # normalized and stats may come precomputed from the matmul that produced value.
    @staticmethod
    def forward(ctx, value, gamma, beta, weight, bias, normalized=None, stats=None):
        if normalized is None:
            normalized, stats = _layer_norm_forward(value, gamma, beta)
        output, weight_bfloat = _linear_forward(normalized, weight, bias)
        ctx.save_for_backward(value, gamma, normalized, stats, weight_bfloat)
        return output, value.view_as(value)

    @staticmethod
    def backward(ctx, grad_output, grad_residual):
        # The second output aliases the input for residual connections; its gradient is added
        # by the layer-norm kernel instead of a separate pass.
        value, gamma, normalized, stats, weight = ctx.saved_tensors
        (grad_value, grad_gamma, grad_beta), grad_weight, grad_bias = _linear_backward(
            normalized, weight, grad_output, norm=(value, gamma, stats, grad_residual),
        )
        return grad_value, grad_gamma, grad_beta, grad_weight, grad_bias, None, None


@_committed
class _FeedForward(torch.autograd.Function):
    # value + second(gelu(first(norm(value)))), with gelu applied in the matmul epilogues. The norm may
    # come precomputed as for _NormLinear, and with next_gamma and next_beta the following layer norm
    # is returned as for _LinearResidual.
    @staticmethod
    def forward(ctx, value, gamma, beta, weight1, bias1, weight2, bias2, normalized=None, stats=None,
                next_gamma=None, next_beta=None):
        if normalized is None:
            normalized, stats = _layer_norm_forward(value, gamma, beta)
        hidden = value.new_empty((len(value), len(weight1)), dtype=torch.bfloat16)
        activated, weight1_bfloat = _linear_forward(normalized, weight1, bias1, hidden=hidden)
        if next_gamma is None:
            output, weight2_bfloat = _linear_forward(activated, weight2, bias2, value)
        else:
            output, weight2_bfloat, next_normalized, next_stats = _linear_residual_norm(
                activated, weight2, bias2, value, next_gamma, next_beta,
            )
            ctx.mark_non_differentiable(next_normalized, next_stats)
        ctx.save_for_backward(value, gamma, normalized, stats, weight1_bfloat, hidden, activated,
                              weight2_bfloat)
        return output if next_gamma is None else (output, next_normalized, next_stats)

    @staticmethod
    def backward(ctx, grad_output, *_):
        value, gamma, normalized, stats, weight1, hidden, activated, weight2 = ctx.saved_tensors
        grad_hidden, grad_weight2, grad_bias2 = _linear_backward(
            activated, weight2, grad_output.float(), hidden,
        )
        (grad_value, grad_gamma, grad_beta), grad_weight1, grad_bias1 = _linear_backward(
            normalized, weight1, grad_hidden, norm=(value, gamma, stats, grad_output),
        )
        return grad_value, grad_gamma, grad_beta, grad_weight1, grad_bias1, grad_weight2, grad_bias2, \
            None, None, None, None


@_committed
class _SparseQKV(torch.autograd.Function):
    # Final-layer projection: keys and values of every row, queries only of the rows at positions
    # (inverse maps rows to their query index or -1). Returns (q, kv, value) like _NormLinear.
    @staticmethod
    def forward(ctx, value, gamma, beta, weight, bias, positions, inverse, normalized=None, stats=None):
        if normalized is None:
            normalized, stats = _layer_norm_forward(value, gamma, beta)
        selected = normalized[positions]
        q, query_weight = _linear_forward(selected, weight[:128], bias[:128])
        kv, kv_weight = _linear_forward(normalized, weight[128:], bias[128:])
        ctx.save_for_backward(value, gamma, normalized, stats, selected, query_weight, kv_weight, inverse)
        return q, kv, value.view_as(value)

    @staticmethod
    def backward(ctx, grad_q, grad_kv, grad_residual):
        value, gamma, normalized, stats, selected, query_weight, kv_weight, inverse = ctx.saved_tensors
        grad_q = grad_q.to(torch.bfloat16).contiguous()
        grad_selected = torch.empty_like(selected, dtype=torch.float32)
        _linear_kernel("linear_input_grad_float_out", grad_q, query_weight, grad_selected, len(selected), 128, 128)
        grad_query_weight, grad_query_bias = _linear_weight_backward(selected, grad_q)
        (grad_value, grad_gamma, grad_beta), grad_kv_weight, grad_kv_bias = _linear_backward(
            normalized, kv_weight, grad_kv, norm=(value, gamma, stats, grad_residual),
            extra=(grad_selected, inverse),
        )
        return grad_value, grad_gamma, grad_beta, torch.cat((grad_query_weight, grad_kv_weight)), \
            torch.cat((grad_query_bias, grad_kv_bias)), None, None, None, None


@_committed
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
        (grad_value, grad_gamma, grad_beta), grad_weight, grad_bias = _linear_backward(
            normalized, weight, grad_output[:, None], norm=(value, gamma, stats, None),
        )
        return grad_value, grad_gamma, grad_beta, grad_weight, grad_bias


def _linear(value, layer, residual=None):
    if value.device.type == "mps" and torch.is_autocast_enabled("mps") and value.shape[1] == 128:
        return (_FastLinear if residual is None else _LinearResidual).apply(
            value, layer.weight, layer.bias, *(() if residual is None else (residual,))
        )
    output = layer(value)
    return output if residual is None else residual + output


def _residual_linear(value, layer, residual, next_norm=None):
    # With next_norm, returns (output, norm) where norm is next_norm's precomputed (bf16 output, stats)
    # or None.
    if next_norm is None:
        return _linear(value, layer, residual)
    if (value.device.type == "mps" and torch.is_autocast_enabled("mps") and value.shape[1] == 128
            and residual.dtype == torch.float32 and len(layer.weight) == 128):
        output, *norm = _LinearResidual.apply(
            value, layer.weight, layer.bias, residual, next_norm.weight, next_norm.bias,
        )
        return output, norm
    return _linear(value, layer, residual), None


def _norm_linear(value, norm, weight, bias, normalized=None):
    if (value.device.type == "mps" and value.dtype == torch.float32
            and torch.is_autocast_enabled("mps") and value.shape[1] == 128):
        return _NormLinear.apply(value, norm.weight, norm.bias, weight, bias, *(normalized or ()))
    return nn.functional.linear(norm(value), weight, bias), value


def _feed_forward(value, norm, first, second, normalized=None, next_norm=None):
    # normalized and next_norm as for _norm_linear and _residual_linear.
    if (value.device.type == "mps" and value.dtype == torch.float32
            and torch.is_autocast_enabled("mps") and value.shape[1] == len(first.weight) == 128):
        output = _FeedForward.apply(
            value, norm.weight, norm.bias, first.weight, first.bias, second.weight, second.bias,
            *(normalized or (None, None)), *(() if next_norm is None else (next_norm.weight, next_norm.bias)),
        )
        return output if next_norm is None else (output[0], list(output[1:]))
    hidden, value = _norm_linear(value, norm, first.weight, first.bias)
    return _residual_linear(nn.functional.gelu(hidden), second, value, next_norm)


def _layer_norm(value, norm):
    if value.device.type == "mps" and torch.is_autocast_enabled("mps") and value.shape[1] == 128:
        return _LayerNorm.apply(value.float().contiguous(), norm.weight, norm.bias)
    return norm(value)


def _norm_head(value, norm, head):
    if (value.device.type == "mps" and value.dtype == torch.float32
            and torch.is_autocast_enabled("mps") and value.shape[1] == 128):
        return _NormHead.apply(value, norm.weight, norm.bias, head.weight, head.bias)
    return head(norm(value)).squeeze(-1)


@_committed
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


@_committed
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


@_committed
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


@_committed
class _FusedDomains(torch.autograd.Function):
    @staticmethod
    def forward(ctx, embedding, *arguments):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        outputs, saved = [], [embedding]
        for categorical, numeric, weight, extra, gamma, beta in zip(*[iter(arguments)] * 6):
            if not len(categorical):
                outputs.append(gamma.new_empty((0, len(gamma))))
                saved.extend((categorical, numeric, weight, gamma,
                              gamma.new_empty((0, len(gamma))), gamma.new_empty((0, 2))))
                continue
            numeric_input = numeric.to(torch.bfloat16)
            numeric = nn.functional.linear(numeric_input, weight.to(torch.bfloat16)) + extra \
                if weight.numel() else extra.to(embedding.dtype)
            rows, fields = categorical.shape
            input = embedding.new_empty((rows, len(gamma)))
            output = torch.empty_like(input)
            stats = embedding.new_empty((rows, 2))
            _RaggedAttention.library.semantic_forward_bfloat(
                categorical, numeric.contiguous(), embedding, gamma, beta, input, output, stats,
                rows, fields, len(gamma), threads=rows * 32, group_size=32,
            )
            outputs.append(output)
            saved.extend((categorical, numeric_input, weight.to(torch.bfloat16), gamma,
                          input, stats))
        ctx.save_for_backward(*saved)
        ctx.extra_dtypes = [extra.dtype for extra in arguments[3::6]]
        return tuple(outputs)

    @staticmethod
    def backward(ctx, *grad_outputs):
        embedding, *saved = ctx.saved_tensors
        grad_embedding = torch.zeros_like(embedding)
        gradients = [grad_embedding]
        for grad_output, values, extra_dtype in zip(grad_outputs, zip(*[iter(saved)] * 6), ctx.extra_dtypes):
            semantic, numeric_input, weight, gamma, input, stats = values
            if not len(semantic):
                gradients.extend((None, None, torch.zeros_like(weight), None,
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
            )[1] if weight.numel() else torch.zeros_like(weight)
            gradients.extend((None, None, grad_weight.float(), grad_input.to(extra_dtype),
                              grad_gamma, grad_beta))
        return tuple(gradients)


class _Assemble(torch.autograd.Function):
    # Padded sequence rows: seeds at offsets, parts at their destinations, zeros after the real tokens.
    @staticmethod
    def forward(ctx, total, offsets, destination, seed, *parts):
        dtype = functools.reduce(torch.promote_types, (part.dtype for part in parts))
        sizes = [len(part) for part in parts]
        current = seed.new_empty((total, seed.shape[-1]), dtype=dtype)
        current[sum(sizes) + len(offsets):] = 0
        current[offsets] = seed.to(dtype)
        for part, rows in zip(parts, destination.split(sizes)):
            current[rows] = part.to(dtype)
        ctx.save_for_backward(offsets, destination)
        ctx.sizes, ctx.seed, ctx.dtypes = sizes, (seed.dim(), seed.dtype), [part.dtype for part in parts]
        return current

    @staticmethod
    def backward(ctx, grad):
        offsets, destination = ctx.saved_tensors
        grad_seed = grad[offsets].to(ctx.seed[1])
        return None, None, None, grad_seed.sum(0) if ctx.seed[0] == 1 else grad_seed, *(
            grad[rows].to(dtype) for rows, dtype in zip(destination.split(ctx.sizes), ctx.dtypes)
        )


@_committed
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


@_committed
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


class ConceptEmbeddings(nn.Module):
    def __init__(self, layout, width):
        super().__init__()
        self.sizes = tuple(layout[name + "_semantic_count"] for name in SEMANTIC_NAMES)
        self.tables = nn.ModuleList(nn.Embedding(size + 1, width, padding_idx=0)
                                    for size in self.sizes)
        self.amounts = nn.ModuleList(nn.Embedding(size + 1, width, padding_idx=0)
                                     for size in self.sizes)
        self.num_embeddings = layout["concept_vocab"]

    def forward(self, codes):
        return nn.functional.embedding(codes, self.flattened())

    def flattened(self, tables=None):
        tables = self.tables if tables is None else tables
        return torch.cat((tables[0].weight[:1] * 0, *(table.weight[1:] for table in tables)))

    def amounts_flattened(self):
        return self.flattened(self.amounts)

    def code(self, name, index):
        table = SEMANTIC_NAMES.index(name)
        return 1 + sum(self.sizes[:table]) + index

    def local(self, name, index):
        return graph_name(self.tables[SEMANTIC_NAMES.index(name)].weight[index + 1], name)


def amount_sum(codes, amounts, table):
    # Each code's amount times that concept's amount embedding, summed per row.
    rows, slots = amounts.nonzero(as_tuple=True)
    values = amounts[rows, slots, None].float() * table[codes[rows, slots].long()].float()
    return values.new_zeros((len(codes), table.shape[1])).index_add_(0, rows, values)


class TokenEncoder(nn.Module):
    def __init__(self, fields, width):
        super().__init__()
        self.numeric = nn.Linear(fields, width, bias=False) if fields else None
        self.norm = nn.LayerNorm(width)

    def forward(self, codes, amounts, fields, embeddings, amount_table):
        if not len(codes):
            return self.norm.weight.new_empty((0, len(self.norm.weight)))
        numeric = self.numeric(fields) if self.numeric is not None else \
            self.norm.weight.new_zeros((len(codes), len(self.norm.weight)))
        numeric = numeric + amount_sum(codes, amounts, amount_table).to(numeric.dtype)
        if (codes.device.type == "mps" and numeric.dtype == torch.bfloat16
                and len(self.norm.weight) % 32 == 0 and len(self.norm.weight) <= 128):
            return _FusedSemantic.apply(codes, numeric, embeddings, self.norm.weight, self.norm.bias)
        return self.norm(nn.functional.embedding(codes.long(), embeddings).sum(1) + numeric)


class Agent(nn.Module):
    def __init__(self, layout, width=128, layers=4, heads=8, feedforward=384, head_width=None,
                 pooling=None, model_version=MODEL_VERSION, potential_weights=None,
                 potential_mode="health_multiplier"):
        nn.Module.__init__(self)
        self.layout = dict(layout)
        if self.layout["version"] != FEATURE_VERSION:
            raise ValueError("incompatible observation layout")
        expected = {name + suffix: value for name, codes, fields in DOMAINS
                    for suffix, value in (("_c", codes), ("_f", fields))}
        expected |= {"domain_count": len(DOMAINS), "place_count": len(PLACES),
                     "globals": RUN_FEATURES, "model_width": width, "model_layers": layers,
                     "model_heads": heads, "model_feedforward": feedforward}
        if any(self.layout.get(key) != value for key, value in expected.items()):
            raise ValueError("incompatible observation schema")
        if min(width, layers, heads, feedforward) <= 0 or width % heads \
                or head_width not in (None, width) \
                or model_version != MODEL_VERSION:
            raise ValueError("invalid architecture")
        self.model_version = model_version
        self.width, self.layers, self.heads, self.feedforward = width, layers, heads, feedforward
        self.head_width = width
        self.state_width = width
        head = potential_head(potential_weights or (0.,) * len(POTENTIAL_TERMS), potential_mode)
        if pooling and set(pooling) - POOLING_DEFAULTS.keys():
            raise ValueError("invalid pooling configuration")
        self.pooling = POOLING_DEFAULTS | (pooling or {})
        for name in COLLECTIONS:
            if self.pooling[name] not in COLLECTION_POOLING:
                raise ValueError(f"invalid {name} pooling")
        for name in ("enemy_effect", "friendly_effect"):
            if self.pooling[name] not in EFFECT_POOLING:
                raise ValueError(f"invalid {name} pooling")
        if self.pooling["continuation"] not in CONTINUATION_POOLING:
            raise ValueError("invalid continuation pooling")
        self.concepts = ConceptEmbeddings(self.layout, width)
        self.encoders = nn.ModuleDict({
            name: TokenEncoder(fields, width)
            for name, _codes, fields in DOMAINS if name not in RAW_DOMAINS
        })
        summary_names = tuple(POOLING_DEFAULTS)
        self.summary_seed = nn.ParameterDict({name: nn.Parameter(torch.randn(width) * .02)
                                              for name in summary_names})
        self.pool_projections = nn.ModuleDict({name: nn.Linear(width, width) for name in POOLS})

        def transformer(layer_count=1):
            layer = nn.TransformerEncoderLayer(
                width, heads, feedforward, dropout=0, batch_first=True,
                norm_first=True, activation="gelu",
            )
            return nn.TransformerEncoder(layer, layer_count, enable_nested_tensor=False)

        self.pool_transformers = nn.ModuleDict()
        for name, mode in self.pooling.items():
            if mode in ("transformer", "transformer_into_actor", "transformer_token"):
                if name not in self.pool_transformers:
                    self.pool_transformers[name] = transformer()
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
        self.graph_out = nn.Linear(width, width)
        self.graph_ff_norm = nn.LayerNorm(width)
        self.graph_ff = nn.Sequential(
            nn.Linear(width, feedforward), nn.GELU(), nn.Linear(feedforward, width),
        )
        self.policy = nn.Linear(width, 1)
        self.critic = nn.Linear(width, 1)
        self._graph_cache = {}
        self._trace_graph = False
        self._sequence_layouts = []
        self._reuse_sequence_layouts = False
        self._sequence_layout_index = 0
        self.cache_stats = {"graph_hit": 0, "graph_miss": 0}
        self.precision = "fp32"
        nn.init.normal_(self.policy.weight, std=.01)
        nn.init.zeros_(self.policy.bias)
        nn.init.zeros_(self.critic.weight)
        nn.init.zeros_(self.critic.bias)
        self.potential = nn.Linear(RUN_FEATURES, 1, bias=False)
        with torch.no_grad():
            self.potential.weight.copy_(torch.tensor([head]))

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
        node_source, levels, offsets, current, current_inverse, keys, rows, sizes, parents = index
        cacheable = not self.training and not self._trace_graph and node_source.device.type == "cpu"
        cached = [self._graph_cache.get(key) for key in keys] if cacheable else [None] * len(keys)
        if cacheable and all(x is not None for x in cached):
            self.cache_stats["graph_hit"] += len(keys)
            nodes = torch.cat(cached)
        else:
            if cacheable:
                self.cache_stats["graph_hit"] += sum(x is not None for x in cached)
                self.cache_stats["graph_miss"] += sum(x is None for x in cached)
            nodes = encoded[node_source]
            if cacheable:
                ranges = offsets.tolist()
                for graph, value in enumerate(cached):
                    if value is not None:
                        nodes[ranges[graph]:ranges[graph + 1]] = value
            # Parents are read before their only update, so one gather serves every level.
            gathered, updates = nodes[rows].split(sizes), []
            for (local, sequence, count, gap), parent_rows, leaves in zip(levels, gathered[::2], gathered[1::2]):
                query, parent = _norm_linear(
                    parent_rows, self.graph_norm, self.graph_query.weight, self.graph_query.bias,
                )
                children = torch.cat((leaves, *updates[len(updates) - gap:]))[local]
                attended = self.attend(query, _norm_linear(
                    children, self.graph_norm, self.graph_key_value.weight, self.graph_key_value.bias,
                )[0], sequence)
                updated = _residual_linear(attended, self.graph_out, parent)
                updated = _feed_forward(updated, self.graph_ff_norm, self.graph_ff[0], self.graph_ff[2])
                updates.append(updated[:count])
            if updates:
                nodes = nodes.index_put((parents,), torch.cat(updates))
            if cacheable:
                offsets = offsets.tolist()
                for graph, key in enumerate(keys):
                    if cached[graph] is None:
                        self._graph_cache[key] = nodes[offsets[graph]:offsets[graph + 1]].detach()
                while len(self._graph_cache) > 8192:
                    self._graph_cache.pop(next(iter(self._graph_cache)))
        return nodes[current][current_inverse], nodes

    def encode_features(self, features, concepts, amounts):
        # One encoding per unique feature row, all domains concatenated (token gid order).
        for (name, *_), values in zip(DOMAINS, features):
            graph_name(values[0], name + " categorical IDs")
            graph_name(values[1], name + " amounts")
            graph_name(values[2], name + " numeric features")
        learned = [(index, name, values) for index, ((name, *_), values) in enumerate(zip(DOMAINS, features))
                   if name in self.encoders]
        if (concepts.device.type == "mps" and torch.is_autocast_enabled("mps")
                and self.width == 128):
            arguments = tuple(
                value
                for _index, name, (codes, values, fields) in learned
                for value in (
                    codes, fields,
                    self.encoders[name].numeric.weight
                    if self.encoders[name].numeric is not None
                    else concepts.new_empty((self.width, 0)),
                    amount_sum(codes, values, amounts).to(torch.bfloat16),
                    self.encoders[name].norm.weight,
                    self.encoders[name].norm.bias,
                )
            )
            outputs = _FusedDomains.apply(concepts, *arguments)
        else:
            outputs = tuple(self.encoders[name](*values, concepts, amounts)
                            for _index, name, values in learned)
        encoded = [None] * len(DOMAINS)
        for output, (index, name, _values) in zip(outputs, learned):
            encoded[index] = graph_name(output, name + " encoding")
        for name in RAW_DOMAINS:
            codes = features[DOMAIN[name]][0]
            encoded[DOMAIN[name]] = concepts.new_zeros((len(codes), self.width)) \
                if name == "map_edge" else concepts[codes[:, 0].long()]
        dtype = outputs[0].dtype if outputs else concepts.dtype
        return graph_name(torch.cat([value.to(dtype) for value in encoded]), "token encodings")

    def _tag(self, values, collection):
        return values + self.concepts.local("collection", collection).to(values.dtype)

    @staticmethod
    def _values(encoded, children, group):
        # Token encodings plus everything summed into them (slot 0: nothing).
        gid, slot, *_ = group
        return encoded[gid] + children[slot].to(encoded.dtype)

    def _sequence(self, values, group, groups, seed, transformer, selected=None, layout=None):
        # values may be a list of parts, in row order of group.
        parts = values if isinstance(values, list) else [values]
        count, device = sum(map(len, parts)), seed.device
        selected_count = selected
        real_tokens = count + groups
        final_attention = transformer is self.global_transformer
        flash = device.type == "mps" and self.width == 128 and self.heads == 4 \
            and torch.is_autocast_enabled("mps") \
            and torch.get_autocast_dtype("mps") == torch.bfloat16
        reuse = flash and getattr(self, "_reuse_sequence_layouts", False)
        if layout is not None:
            offsets, destination, sequence, block_rows, block_starts, metal_offsets, \
                metal_lengths, sparse = layout
        elif reuse:
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
        if device.type == "mps" and self.width // self.heads in (16, 32) \
                and layout is None and not reuse:
            lengths = counts + 1
            padded_groups = (groups // 256 + 1) * 256
            lengths = nn.functional.pad(lengths, (0, padded_groups - groups), value=1)
            total = count + padded_groups
            extra = (-total) % 16_384
            lengths[groups:] += extra // (padded_groups - groups)
            lengths[groups:groups + extra % (padded_groups - groups)] += 1
            offsets = torch.cat((counts.new_zeros(1), lengths.cumsum(0)))
            starts = torch.repeat_interleave(offsets[:groups], counts)
            position = torch.arange(count, device=device) \
                - torch.repeat_interleave(counts.cumsum(0) - counts, counts) + 1
            destination = (starts + position)[inverse]
            sequence = torch.repeat_interleave(
                torch.arange(len(lengths), device=device), lengths,
            )
            if flash:
                blocks = (lengths[:groups] + 31) // 32
                block_rows = torch.repeat_interleave(
                    torch.arange(groups, device=device), blocks,
                ).to(torch.int32)
                block_starts = torch.cat((counts.new_zeros(1), blocks.cumsum(0)[:-1])).to(
                    torch.int32
                )
                metal_offsets = offsets.to(torch.int32)
                metal_lengths = lengths.to(torch.int32)
            sparse = None
            if selected_count is not None:
                positions, query_order = torch.cat((
                    offsets[:groups], destination[-selected_count:],
                )).sort()
                query_counts = torch.bincount(sequence[positions], minlength=len(lengths))
                query_offsets = torch.cat((query_counts.new_zeros(1), query_counts.cumsum(0)))
                item_positions = torch.empty_like(query_order)
                item_positions[query_order] = torch.arange(len(query_order), device=device)
                sparse = positions, item_positions[groups:], query_offsets
            if flash and self.training and torch.is_grad_enabled():
                self._sequence_layouts.append((
                    order, inverse, selected, counts, lengths, offsets, destination, sequence,
                    block_rows, block_starts, metal_offsets, metal_lengths, sparse,
                ))
        if device.type == "mps" and self.width // self.heads in (16, 32):
            current = _Assemble.apply(len(sequence), offsets[:groups], destination, seed, *parts)

            # Each residual matmul also computes the next layer norm, passed on as normalized.
            def attend_layer(current, layer, selected, query_offsets, sparse, normalized):
                weights = layer.self_attn.in_proj_weight, layer.self_attn.in_proj_bias
                if flash and sparse:
                    inverse = torch.full((len(current),), -1, dtype=torch.int32, device=device)
                    inverse[selected] = torch.arange(len(selected), dtype=torch.int32, device=device)
                    q, kv, current = _SparseQKV.apply(
                        current, layer.norm1.weight, layer.norm1.bias, *weights, selected, inverse,
                        *(normalized or ()),
                    )
                    attended = _SparseAttention.apply(q, kv, metal_offsets, query_offsets.to(torch.int32))
                    return _residual_linear(attended, layer.self_attn.out_proj, current[selected],
                                            layer.norm2)
                qkv, current = _norm_linear(current, layer.norm1, *weights, normalized)
                fp32_value_backward = final_attention \
                    and layer_index + 1 == len(transformer.layers) \
                    and self.precision == "bf16_with_fp32_dV"
                if fp32_value_backward:
                    attended = _RaggedAttention.apply(
                        qkv, offsets.to(torch.int32), sequence.to(torch.int32),
                        layer.self_attn.num_heads, selected.to(torch.int32),
                        query_offsets.to(torch.int32), sparse, True,
                    )
                elif flash and not sparse:
                    attended = _FlashAttention.apply(
                        qkv, metal_offsets, metal_lengths, block_rows, block_starts, real_tokens,
                    )
                else:
                    attended = _RaggedAttention.apply(
                        qkv, offsets.to(torch.int32), sequence.to(torch.int32),
                        layer.self_attn.num_heads, selected.to(torch.int32),
                        query_offsets.to(torch.int32), sparse, False,
                    )
                residual = current[selected] if sparse else current
                return _residual_linear(attended, layer.self_attn.out_proj, residual, layer.norm2)

            normalized = None
            for layer_index, layer in enumerate(transformer.layers):
                if selected_count is not None and layer_index == len(transformer.layers) - 1:
                    positions, item_positions, query_offsets = sparse
                    current, normalized = attend_layer(
                        current, layer, positions, query_offsets, True, normalized,
                    )
                    current = _feed_forward(current, layer.norm2, layer.linear1, layer.linear2, normalized)
                    state = current[query_offsets[:groups]]
                    items = current[item_positions]
                    return state, items
                current, normalized = attend_layer(current, layer, sequence, offsets, False, normalized)
                following = transformer.layers[layer_index + 1].norm1 \
                    if layer_index + 1 < len(transformer.layers) else None
                current = _feed_forward(current, layer.norm2, layer.linear1, layer.linear2, normalized,
                                        following)
                current, normalized = current if following is not None else (current, None)
            items = current[destination] if count else parts[0]
            return current[offsets[:groups]], items
        values = torch.cat(parts)[order]
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
        if mode == "sum":
            summary = _GroupSum.apply(values, group, groups) \
                if values.device.type == "mps" and len(values) else \
                values.new_zeros((groups, self.width)).index_add_(
                    0, group, values.to(torch.float32),
                )
        else:
            summary, _ = self._sequence(values, group, groups, self.summary_seed[name],
                                        self.pool_transformers[transformer_name or name])
        return summary

    def _collection(self, values, rows, active, name):
        collection = ("deck", "hand", "draw", "discard", "exhaust", "relic", "potion", "orb").index(name)
        mode = self.pooling[name]
        if mode == "global_tokens":
            return self._tag(values, collection), rows
        summary = self._summarize(values, rows, int(active.max().item()) + 1 if len(active) else 0,
                                  name, mode)
        return self._tag(summary[active], collection), active

    def _actors(self, encoded, children, concepts, plan):
        actor_group, move_codes, move_order, move_packed_order, move_present, move_batch_sizes, \
            move_terminal, effect_group, families = plan
        rows = actor_group[2]
        actors = self._values(encoded, children, actor_group)
        if not len(actors):
            return actors, rows, [], []
        if len(move_codes):
            move_values = concepts[move_codes]
            move_values = move_values[move_order][move_packed_order] \
                if move_terminal is None else move_values
            if move_values.device.type == "mps" and torch.is_autocast_enabled("mps"):
                output = self._packed_gru(
                    move_values, move_batch_sizes, self.move_gru,
                    None if move_terminal is None else move_packed_order,
                )[None]
                if move_terminal is not None:
                    output = output[:, move_terminal]
            else:
                packed = nn.utils.rnn.PackedSequence(move_values, torch.tensor(move_batch_sizes))
                _, output = self.move_gru(packed)
            move = move_values.new_zeros((len(actors), self.width)).index_put(
                (move_present,), output[0],
            )
            actors = actors + move
        effects = self._values(encoded, children, effect_group)
        extra_values, extra_rows = [], []
        for name, (actor_index, effect_source, effect_actor, local_group) in zip(
            ("friendly_effect", "enemy_effect"), families,
        ):
            if not len(actor_index):
                continue
            values = effects[effect_source]
            mode = self.pooling[name]
            if mode == "global_tokens":
                extra_values.append(self._tag(values, 8 + (name == "enemy_effect")))
                extra_rows.append(rows[effect_actor])
                continue
            pooled_mode = "transformer" if mode.startswith("transformer") else "sum"
            summary = self._summarize(values, local_group, len(actor_index), name, pooled_mode)
            if mode.endswith("into_actor"):
                actors = actors.index_put((actor_index,), summary.to(actors.dtype), accumulate=True)
            else:
                extra_values.append(self._tag(summary, 8 + (name == "enemy_effect")))
                extra_rows.append(rows[actor_index])
        actors = _layer_norm(actors, self.actor_norm)
        return actors, rows, extra_values, extra_rows

    def _packed_gru(self, values, batch_sizes, gru, parents=None):
        gates = _FastLinear.apply(values, gru.weight_ih_l0, gru.bias_ih_l0).split(batch_sizes)
        if parents is not None:
            levels = []
            for level_gates, level_parents in zip(gates, parents.split(batch_sizes)):
                previous = values.new_zeros((len(level_gates), self.width)) if not levels \
                    else levels[-1][level_parents]
                hidden = _FastLinear.apply(previous, gru.weight_hh_l0, gru.bias_hh_l0)
                levels.append(_GruStep.apply(level_gates, hidden, previous))
            return torch.cat(levels)
        state = torch.zeros((batch_sizes[0], self.width), device=values.device)
        for level_gates in gates:
            size = len(level_gates)
            hidden = _FastLinear.apply(state[:size], gru.weight_hh_l0, gru.bias_hh_l0)
            current = _GruStep.apply(level_gates, hidden, state[:size])
            state = torch.cat((current, state[size:])) if size < len(state) else current
        return state

    def encode_state(self, encoded, plan, map_index, action_index, concepts, batch):
        children = encoded.new_zeros((plan["receivers"], self.width), dtype=torch.float32).index_add_(
            0, plan["child_slot"], encoded[plan["child_gid"]].float(),
        )
        current, nodes = self.encode_map(encoded, map_index)
        action_row, action_flat, _legal, policy_sequence, actions, path_rows, path_nodes, \
            action_group, global_layout, target_rows, target_group = action_index
        action = self._values(encoded, children, action_group)
        action = action.index_add(0, path_rows, nodes[path_nodes].to(action.dtype))
        action = action.index_add(0, target_rows, self._values(encoded, children, target_group))
        action = _layer_norm(action, self.action_norm)
        values, rows = [], []
        segments = getattr(self, "_explanation_segments", None)
        everywhere = torch.arange(batch, device=encoded.device)

        def add(value, row, name):
            values.append(graph_name(value, name)); rows.append(row)
            if segments is not None:
                segments.append((name, len(value)))

        run = self._values(encoded, children, plan["run"])
        seed = graph_name(run.new_zeros((batch, self.width)).index_put((plan["run"][2],), run), "run")
        add(self._values(encoded, children, plan["phase"]), plan["phase"][2], "phase")
        add(current + self.concepts.local("token_role", 0).to(current.dtype), everywhere, "map")
        for name, (source, pooled) in zip(POOLS, plan["pools"]):
            if encoded.device.type == "mps":
                pooled_gid, item_rows, inverse, groups = pooled
                summary = _IndexedGroupSum.apply(encoded, pooled_gid, item_rows, groups)[inverse]
            else:
                summary = self._summarize(encoded[source[0]], source[2], batch, name, "sum")
            add(_linear(summary, self.pool_projections[name]), everywhere, name)
        actors, actor_rows, effect_values, effect_rows = self._actors(
            encoded, children, concepts, plan["actors"],
        )
        for name in COLLECTIONS:
            group = plan[name]
            item, item_rows = self._collection(
                self._values(encoded, children, group), group[2], group[3], name,
            )
            add(item, item_rows, name)
        add(actors, actor_rows, "actor")
        for value, row in zip(effect_values, effect_rows):
            add(value, row, "effect")
        continuation_group, maximum, position, present, last, packed_order, packed_present, \
            batch_sizes = plan["continuation"]
        continuation = self._values(encoded, children, continuation_group)
        continuation_rows = continuation_group[2]
        mode = self.pooling["continuation"]
        if mode == "global_tokens":
            add(self._tag(continuation, 10), continuation_rows, "continuation")
        elif mode == "gru":
            if len(continuation):
                if continuation.device.type == "mps" and torch.is_autocast_enabled("mps"):
                    state = self._packed_gru(
                        continuation[packed_order], batch_sizes, self.continuation_gru,
                    )
                    summary = continuation.new_zeros((batch, self.width)).index_put(
                        (packed_present,), state,
                    )
                else:
                    padded = continuation.new_zeros((batch, maximum, self.width))
                    padded[continuation_rows, position] = continuation
                    output, _ = self.continuation_gru(padded)
                    summary = continuation.new_zeros((batch, self.width)).index_put(
                        (present,), output[present, last],
                    )
            else:
                summary = continuation.new_zeros((batch, self.width))
            add(self._tag(summary, 10), everywhere, "continuation")
        else:
            add(self._tag(self._summarize(
                    continuation, continuation_rows, batch, "continuation", mode), 10),
                everywhere, "continuation")
        add(self._values(encoded, children, plan["crystal"]), plan["crystal"][2], "crystal")
        add(action, action_row, "action")

        if global_layout is not None and len(global_layout[1]) != sum(map(len, values)):
            raise RuntimeError("invalid precomputed sequence layout")
        rows = None if global_layout is not None else torch.cat(rows)
        self._sequence_lengths = global_layout[6][:batch] if global_layout is not None else \
            torch.bincount(rows, minlength=batch) + 1
        state, transformed = self._sequence(
            values, rows, batch, seed, self.global_transformer,
            len(action), global_layout,
        )
        return state, transformed if len(action) else action, \
            action_row, action_flat, actions, policy_sequence

    def forward(self, character, globals_, features, plan, map_index, action_index,
                return_state=False, policy_only=False, flat_policy=False, temperature=1):
        if self.training and torch.is_grad_enabled():
            self._sequence_layouts.clear()
        if self._reuse_sequence_layouts:
            self._sequence_layout_index = 0
        concepts = graph_name(self.concepts.flattened(), "concepts")
        amounts = graph_name(self.concepts.amounts_flattened(), "amount embeddings")
        encoded = self.encode_features(features, concepts, amounts)
        state, action, action_row, action_flat, actions, sequence = self.encode_state(
            encoded, plan, map_index, action_index, concepts, len(character),
        )
        scores = _norm_head(action, self.global_norm, self.policy) / temperature
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
        critic = _norm_head(state, self.global_norm, self.critic) + self.potential_value(globals_)
        output = policy, critic[:, None]
        return (*output, self.global_norm(state)) if return_state else output

    def potential_value(self, globals_):
        with torch.autocast(globals_.device.type, enabled=False):
            return self.potential(globals_.float()).squeeze(-1)


def architecture(model):
    return {
        "width": model.width, "layers": model.layers, "heads": model.heads,
        "feedforward": model.feedforward, "head_width": model.head_width,
        "domains": [name for name, *_ in DOMAINS], "places": list(PLACES),
        "globals": model.layout["globals"], "collections": list(COLLECTIONS),
        "state_width": model.state_width, "policy_input": model.width,
        "map_layers": 1, "global_layers": model.layers, "local_layers": 1,
        "attention": "fully_bidirectional",
        "activation": "gelu", "dropout": 0,
        "token_encoder": "LayerNorm(sum(E[code])+sum(amount*A[code])+Linear(fields,bias=False))",
        "children": "summed into their parent token",
        "sum_pooling": "unnormalized",
        "policy_factorization": "one linear logit per transformed legal action token",
        "policy_entropy": ["candidate", "normalized", "effective_actions"],
        "map": "reverse-topological sparse attention; current node is the map summary",
        "move_history": f"one-layer {model.width}-D GRU",
        "continuations": "normalized execution queue",
        "candidate_context": "attached tokens summed into the candidate, plus its map node",
        "fixed_tokens": ["run_state", "current_phase", "current_map_node", "removed_relics",
                         "encounter_pool", "event_pool"],
        "concept_vocab": model.concepts.num_embeddings,
        "semantic_tables": dict(zip(SEMANTIC_NAMES, model.concepts.sizes)),
        "position_caps": POSITION_CAPS,
        "pooling": model.pooling,
        "value_heads": ["terminal_floor_scalar"],
        "winning_reservoir": WINNING_CAPACITY,
    }


def predict(model, inputs, precision, temperature=1, policy_only=False, flat_policy=False):
    if precision not in TRAINING_PRECISIONS:
        raise ValueError("invalid precision")
    kind = inputs[0].device.type
    if precision != "fp32" and kind not in ("mps", "cuda"):
        raise ValueError(f"{precision} requires MPS or CUDA")
    dtype = torch.bfloat16 if precision != "fp32" else torch.float16
    with torch.autocast(kind, dtype=dtype, enabled=precision != "fp32"):
        output = model(
            *inputs[:6], temperature=temperature, policy_only=policy_only,
            flat_policy=flat_policy,
        )
    return output.float() if policy_only else tuple(value.float() for value in output)


def predict_cached(model, inputs, precision, temperature=1):
    globals_, state, action, offsets, action_rows = inputs
    kind = state.device.type
    dtype = torch.bfloat16 if precision != "fp32" else torch.float16
    with torch.autocast(kind, dtype=dtype, enabled=precision != "fp32"):
        scores = model.policy(action).squeeze(-1) / temperature
        policy = model.group_log_softmax(
            scores, torch.ones_like(scores, dtype=torch.bool),
            (offsets, action_rows, None, None),
        )
        critic = model.critic(state).squeeze(-1) + model.potential_value(globals_)
    return policy.float(), critic.float()[:, None]


def cached_tensors(features, width):
    values = [np.frombuffer(row, "<f4") for row in features]
    rows = [row[RUN_FEATURES:].reshape(-1, width) for row in values]
    lengths = np.asarray([len(row) - 1 for row in rows], np.int32)
    offsets = np.r_[0, np.cumsum(lengths, dtype=np.int64)].astype(np.int32)
    return (
        torch.from_numpy(np.stack([row[:RUN_FEATURES] for row in values])),
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


def sequence_layout(group, groups, selected, target):
    group = np.asarray(group, np.int64)
    order = np.argsort(group, kind="stable")
    inverse = np.empty_like(order)
    inverse[order] = np.arange(len(order))
    counts = np.bincount(group, minlength=groups)
    lengths = counts + 1
    padded_groups = (groups // 256 + 1) * 256
    lengths = np.pad(lengths, (0, padded_groups - groups), constant_values=1)
    total = len(group) + padded_groups
    extra = (-total) % 16_384
    lengths[groups:] += extra // (padded_groups - groups)
    lengths[groups:groups + extra % (padded_groups - groups)] += 1
    offsets = np.r_[0, np.cumsum(lengths)]
    starts = np.repeat(offsets[:groups], counts)
    position = np.arange(len(group)) - np.repeat(np.cumsum(counts) - counts, counts) + 1
    destination = (starts + position)[inverse]
    sequence = np.repeat(np.arange(len(lengths)), lengths)
    blocks = (lengths[:groups] + 31) // 32
    block_rows = np.repeat(np.arange(groups), blocks).astype(np.int32)
    block_starts = np.r_[0, np.cumsum(blocks)[:-1]].astype(np.int32)
    sparse = None
    if selected is not None:
        queries = np.r_[offsets[:groups], destination[-selected:]]
        query_order = np.argsort(queries, kind="stable")
        positions = queries[query_order]
        query_counts = np.bincount(sequence[positions], minlength=len(lengths))
        query_offsets = np.r_[0, np.cumsum(query_counts)]
        item_positions = np.empty_like(query_order)
        item_positions[query_order] = np.arange(len(query_order))
        sparse = positions, item_positions[groups:], query_offsets
    tensor = lambda value, dtype=None: torch.as_tensor(value, dtype=dtype, device=target)
    return (
        tensor(offsets), tensor(destination), tensor(sequence), tensor(block_rows),
        tensor(block_starts), tensor(offsets, torch.int32), tensor(lengths, torch.int32),
        None if sparse is None else tuple(tensor(value) for value in sparse),
    )


def observation_legal(observation):
    return np.asarray(observation[2], bool)


def prefix_tree(tokens, counts):
    present = np.flatnonzero(counts)
    starts = np.cumsum(counts) - counts
    previous = np.zeros(len(counts), np.int64)
    sources, parents, sizes = [], [], []
    terminal = np.empty(len(present), np.int64)
    offset = 0
    radix = int(tokens.max(initial=0)) + 1
    for position in range(counts.max(initial=0)):
        active = present[counts[present] > position]
        source = starts[active] + position
        keys = previous[active] * radix + tokens[source]
        _unique, first, inverse = np.unique(keys, return_index=True, return_inverse=True)
        sources.extend(source[first]); parents.extend(previous[active][first])
        previous[active] = inverse
        terminal[counts[present] == position + 1] = offset + inverse[
            counts[active] == position + 1
        ]
        sizes.append(len(first)); offset += len(first)
    return tuple(map(lambda value: np.asarray(value, np.int64), (
        sources, parents, terminal,
    ))), tuple(sizes)


def packed_order(counts):
    # Rows by decreasing length, then items interleaved by position (PackedSequence order).
    present = np.flatnonzero(counts)
    group = np.repeat(np.arange(len(present)), counts[present])
    position = np.arange(len(group)) - np.repeat(np.cumsum(counts[present]) - counts[present], counts[present])
    length_order = np.argsort(-counts[present], kind="stable")
    length_inverse = np.empty_like(length_order)
    length_inverse[length_order] = np.arange(len(length_order))
    order = np.lexsort((length_inverse[group], position))
    sizes = tuple(int((counts[present] > index).sum()) for index in range(counts.max(initial=0)))
    return present[length_order], order, sizes


def model_inputs(character, globals_, features, tokens, legal, target, model, pad=None):
    batch, actions = len(character), legal.shape[1]
    globals_ = np.asarray(globals_, np.float32)
    if globals_.shape != (batch, model.layout["globals"]) or not np.isfinite(globals_).all():
        raise ValueError("invalid public globals")
    pad = model.training and model.global_norm.weight.device.type == "mps" if pad is None else pad
    bucket = lambda count: 0 if not count else min(
        size for power in range(max(1, (count - 1).bit_length()), 64)
        for size in (3 * (1 << power) // 4, 1 << power) if size >= count
    )
    tensor = lambda value: torch.as_tensor(np.asarray(value), dtype=torch.long, device=target)
    domain, place, parent, link, row, feature = (np.asarray(value) for value in tokens)
    count = len(domain)
    feature_tensors, sizes = [], []
    for (name, codes, fields), (c, a, f) in zip(DOMAINS, features):
        c, a, f = np.asarray(c), np.asarray(a, np.float32), np.asarray(f, np.float32)
        if c.shape[1:] != (codes,) or f.shape[1:] != (fields,) or a.shape != c.shape:
            raise ValueError(f"invalid {name} schema")
        if c.max(initial=0) >= model.concepts.num_embeddings:
            raise ValueError(f"invalid {name} semantic id")
        if not (np.isfinite(a).all() and np.isfinite(f).all()):
            raise ValueError(f"invalid {name} numbers")
        size = bucket(len(c)) if pad else len(c)
        padding = ((0, size - len(c)), (0, 0))
        feature_tensors.append(tuple(torch.as_tensor(value, device=target) for value in (
            np.pad(c, padding).astype(np.int32), np.pad(a, padding), np.pad(f, padding),
        )))
        sizes.append(size)
    gid = np.r_[0, np.cumsum(sizes)][:-1][domain] + feature
    starts = np.searchsorted(row, np.arange(batch))
    at = lambda name: np.flatnonzero(place == PLACE[name])
    children = at("child")
    receivers, receiver_slot = np.unique(parent[children], return_inverse=True)
    slot = np.zeros(count, np.int64)
    slot[receivers] = np.arange(1, len(receivers) + 1)
    group = lambda index: (tensor(gid[index]), tensor(slot[index]), tensor(row[index]))
    all_rows = np.arange(batch)

    nodes, edges = at("map_node"), at("map_edge")
    node_counts = np.bincount(row[nodes], minlength=batch)
    node_offsets = np.r_[0, np.cumsum(node_counts)]
    if np.any(nodes - starts[row[nodes]] != np.arange(len(nodes)) - node_offsets[row[nodes]]):
        raise ValueError("map nodes must lead their row")
    edge_offsets = np.r_[0, np.cumsum(np.bincount(row[edges], minlength=batch))]
    local_edges = np.stack((parent[edges], link[edges]), 1) - starts[row[edges]][:, None]
    if np.any(local_edges < 0) or np.any(local_edges >= node_counts[row[edges]][:, None]):
        raise ValueError("map edge references missing node")
    node_group = lambda index: nodes[node_offsets[index]:node_offsets[index + 1]]
    edge_group = lambda index: local_edges[edge_offsets[index]:edge_offsets[index + 1]]
    if model.training:
        representatives, inverse = map(np.asarray, sts2_sim.unique_graphs(
            feature[nodes], node_offsets, local_edges.reshape(-1), edge_offsets,
        ))
        graph_keys = ()
    else:
        node_codes = np.asarray(features[DOMAIN["map_node"]][0])
        graph_keys = tuple((node_codes[feature[node_group(index)]].tobytes(), edge_group(index).tobytes())
                           for index in range(batch))
        unique, representatives, inverse = {}, [], np.empty(batch, np.int64)
        for index, key in enumerate(graph_keys):
            if key not in unique:
                unique[key] = len(representatives); representatives.append(index)
            inverse[index] = unique[key]
        representatives = np.asarray(representatives, np.int64)
    rep_counts = node_counts[representatives]
    rep_starts = np.r_[0, np.cumsum(rep_counts)]
    node_source = np.concatenate([node_group(index) for index in representatives] or [np.empty(0, np.int64)])
    graph_for_node = np.repeat(np.arange(len(representatives)), rep_counts)
    missing = np.asarray([
        model.training or target.type != "cpu" or graph_keys[index] not in model._graph_cache
        for index in representatives
    ], bool)
    edge_src, edge_dst = (np.concatenate([rep_starts[graph] + edge_group(index)[:, side]
                                          for graph, index in enumerate(representatives)]
                                         or [np.empty(0, np.int64)]) for side in (0, 1))
    topo = np.zeros(len(node_source), np.int64)
    for _ in range(len(node_source)):
        previous = topo.copy()
        np.maximum.at(topo, edge_dst, topo[edge_src] + 1)
        if np.array_equal(topo, previous):
            break
    if len(edge_src) and np.any(topo[edge_dst] <= topo[edge_src]):
        raise ValueError("map is not a forward DAG")
    levels = []
    for level in sorted(np.unique(topo[edge_src]), reverse=True):
        selected = np.flatnonzero(topo[edge_src] == level)
        selected = selected[np.argsort(edge_src[selected], kind="stable")]
        parents, local = np.unique(edge_src[selected], return_inverse=True)
        selected = selected[missing[graph_for_node[parents]][local]]
        if len(selected):
            parents, local = np.unique(edge_src[selected], return_inverse=True)
            levels.append((parents, edge_dst[selected], local))
    # Children are leaves or nodes updated by recent levels: each level reads them from a small pool of
    # its leaves followed by the outputs of the previous `gap` levels.
    node_level, node_position = np.full(len(node_source), -1), np.zeros(len(node_source), np.int64)
    for level, (parents, _children, _local) in enumerate(levels):
        node_level[parents], node_position[parents] = level, np.arange(len(parents))
    map_rows, map_sizes, padded = [], [], []
    for level, (parents, targets, local) in enumerate(levels):
        child_level = node_level[targets]
        leaves = np.unique(targets[child_level < 0])
        gap = level - int(child_level[child_level >= 0].min(initial=level))
        offsets = np.cumsum([len(leaves)] + [len(levels[previous][0]) for previous in range(level - gap, level)])
        position = np.where(child_level < 0, np.searchsorted(leaves, targets),
                            offsets[np.clip(child_level - level + gap, 0, None)] + node_position[targets])
        parent_count, edge_count = len(parents), len(targets)
        edge_size = bucket(edge_count) if pad else edge_count
        parent_size = bucket(parent_count + (edge_size > edge_count)) if pad else parent_count
        extra = edge_size - edge_count
        map_rows += [np.pad(parents, (0, parent_size - parent_count)), leaves]
        map_sizes += [parent_size, len(leaves)]
        padded.append((
            tensor(np.pad(position, (0, extra))),
            candidate_index(np.r_[local, np.full(extra, parent_count, np.int32)], parent_size, target,
                            bool(pad)),
            parent_count, gap,
        ))
    map_parents = np.concatenate([parents for parents, _targets, _local in levels] or [np.empty(0, np.int64)])
    node_position_of = lambda index, token: rep_starts[inverse[row[index]]] + token - starts[row[index]]
    run = at("run")
    if not np.array_equal(row[run], all_rows):
        raise ValueError("each observation needs one run token")
    if np.any(link[run] < 0) or np.any(link[run] - starts[row[run]] >= node_counts[row[run]]):
        raise ValueError("current map node is missing")
    current, current_inverse = np.unique(node_position_of(run, link[run]), return_inverse=True)
    map_index = (
        tensor(gid[node_source]), tuple(padded), tensor(rep_starts), tensor(current),
        tensor(current_inverse), tuple(graph_keys[index] for index in representatives) if graph_keys else (),
        tensor(np.concatenate(map_rows or [np.empty(0, np.int64)])), tuple(map_sizes), tensor(map_parents),
    )

    candidates = at("candidate")
    action_row = row[candidates]
    action_counts = np.bincount(action_row, minlength=batch)
    action_position = np.arange(len(candidates)) - np.repeat(np.cumsum(action_counts) - action_counts,
                                                             action_counts)
    action_flat = action_row * actions + action_position
    action_legal = legal[action_row, action_position]
    links = link[candidates]
    linked_place = np.where(links >= 0, place[links].astype(np.int64), -1)
    path_rows = np.flatnonzero(linked_place == PLACE["map_node"])
    target_rows = np.flatnonzero((links >= 0) & (linked_place != PLACE["map_node"]))
    path_tokens = candidates[path_rows]

    pools = []
    for name in POOLS:
        source = at(name)
        offsets = np.r_[0, np.cumsum(np.bincount(row[source], minlength=batch))]
        pooled, item_rows, pool_inverse = map(np.asarray, sts2_sim.deduplicate_sequences(gid, source, offsets))
        groups = int(pool_inverse.max(initial=-1)) + 1
        pools.append((group(source),
                      (tensor(gid[pooled]), tensor(item_rows), tensor(pool_inverse), groups)))

    actor_tokens = np.flatnonzero(np.isin(place, [PLACE["player"], PLACE["ally"], PLACE["enemy"]]))
    actor_rows = row[actor_tokens]
    combat_rows = row[at("player")]
    actor_of = np.full(count, -1)
    actor_of[actor_tokens] = np.arange(len(actor_tokens))
    moves = at("move")
    move_group = actor_of[parent[moves]]
    if np.any(move_group < 0):
        raise ValueError("move token without actor")
    move_codes = np.asarray(features[DOMAIN["move"]][0])[feature[moves], 0].astype(np.int64)
    move_order = np.argsort(move_group, kind="stable")
    move_codes, move_group = move_codes[move_order], move_group[move_order]
    move_counts = np.bincount(move_group, minlength=len(actor_tokens))
    move_present = np.flatnonzero(move_counts)
    move_terminal = None
    if pad and model.precision != "fp32" and len(move_present):
        (sources, move_packed_order, move_terminal), move_batch_sizes = prefix_tree(move_codes, move_counts)
        move_codes, move_order = move_codes[sources], None
    else:
        move_present, move_packed_order, move_batch_sizes = packed_order(move_counts)
        move_order = np.arange(len(move_codes))
    effects = at("effect")
    effect_actor = actor_of[parent[effects]]
    if np.any(effect_actor < 0):
        raise ValueError("effect token without actor")
    families, effect_output_rows = [], []
    for name, family in zip(("friendly_effect", "enemy_effect"),
                            (place[actor_tokens] != PLACE["enemy"], place[actor_tokens] == PLACE["enemy"])):
        actor_index = np.flatnonzero(family)
        lookup = np.full(len(actor_tokens), -1, np.int64)
        lookup[actor_index] = np.arange(len(actor_index))
        selected = lookup[effect_actor] >= 0
        effect_source, owners = np.flatnonzero(selected), effect_actor[selected]
        families.append((tensor(actor_index), tensor(effect_source), tensor(owners), tensor(lookup[owners])))
        if len(actor_index) and model.pooling[name] == "global_tokens":
            effect_output_rows.append(actor_rows[owners])
        elif len(actor_index) and not model.pooling[name].endswith("into_actor"):
            effect_output_rows.append(actor_rows[actor_index])
    actor_plan = (
        group(actor_tokens), tensor(move_codes), None if move_order is None else tensor(move_order),
        tensor(move_packed_order), tensor(move_present), move_batch_sizes,
        None if move_terminal is None else tensor(move_terminal), group(effects), tuple(families),
    )

    heads = at("continuation")
    head_rows = row[heads]
    head_counts = np.bincount(head_rows, minlength=batch)
    present = np.flatnonzero(head_counts)
    position = np.arange(len(heads)) - np.repeat(np.cumsum(head_counts) - head_counts, head_counts)
    packed_present, continuation_order, batch_sizes = packed_order(head_counts)
    continuation_plan = (
        group(heads), int(head_counts.max(initial=0)), tensor(position), tensor(present),
        tensor(head_counts[present] - 1), tensor(continuation_order), tensor(packed_present), batch_sizes,
    )

    plan = {
        "child_gid": tensor(gid[children]), "child_slot": tensor(receiver_slot + 1),
        "receivers": len(receivers) + 1, "run": group(run), "phase": group(at("phase")),
        "pools": tuple(pools), "actors": actor_plan, "continuation": continuation_plan,
        "crystal": group(at("crystal")),
    }
    for name in COLLECTIONS:
        active = all_rows if name in ("deck", "relic", "potion") else combat_rows
        plan[name] = (*group(at(name)), tensor(active))

    phase_rows = row[at("phase")]
    global_rows = [phase_rows, *([all_rows] * 4)]
    for name in COLLECTIONS:
        source = at(name)
        active = all_rows if name in ("deck", "relic", "potion") else combat_rows
        global_rows.append(row[source] if model.pooling[name] == "global_tokens" else active)
    global_rows.extend((actor_rows, *effect_output_rows))
    global_rows.append(head_rows if model.pooling["continuation"] == "global_tokens" else all_rows)
    global_rows.append(row[at("crystal")])
    global_rows.append(action_row)
    global_layout = sequence_layout(
        np.concatenate(global_rows), batch, len(candidates), target,
    ) if pad else None
    action_index = (
        tensor(action_row), tensor(action_flat), torch.as_tensor(action_legal, device=target),
        candidate_index(action_row, batch, target, True), actions, tensor(path_rows),
        tensor(node_position_of(path_tokens, link[path_tokens])), group(candidates), global_layout,
        tensor(target_rows), group(links[target_rows]),
    )
    return (
        torch.as_tensor(character, dtype=torch.long, device=target),
        torch.as_tensor(globals_, device=target), tuple(feature_tensors), plan, map_index,
        action_index, torch.as_tensor(legal, device=target),
    )


def validate_packed(row):
    if not isinstance(row, bytes):
        raise ValueError("packed observation must be compact bytes")
    sts2_sim.validate_compact_observation(row)


def packed_character(row):
    return row[4]


def packed_action_count(row):
    return struct.unpack_from("<I", row, 16)[0]


def packed_legal_count(row):
    return struct.unpack_from("<I", row, 20)[0]


def packed_state_count(row):
    return struct.unpack_from("<I", row, 24)[0]


def _pack_batch(observation):
    return list(observation[4])


def pack_batch(observation, validate=True):
    rows = _pack_batch(observation)
    if validate:
        for row in filter(None, rows):
            validate_packed(row)
    return rows


def tensors(observation, target, model):
    return unpack(_pack_batch(observation), target, model)


def unpack(rows, target, model, validate=True, pad=None):
    if validate:
        for row in rows:
            validate_packed(row)
    character, globals_, features, tokens, legal = sts2_sim.unpack_packed_observations(list(rows))
    return model_inputs(
        np.asarray(character), np.asarray(globals_), [tuple(map(np.asarray, value)) for value in features],
        tuple(map(np.asarray, tokens)), np.asarray(legal), target, model, pad,
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
    legacy = (version, checkpoint.get("feature_version")) == (77, 59)
    if checkpoint.get("schema") != 1 or version != MODEL_VERSION and not legacy:
        raise ValueError("incompatible checkpoint")
    config = checkpoint.get("architecture", {})
    try:
        dimensions = tuple(config[key] for key in ("width", "layers", "heads", "feedforward"))
        live = sts2_sim.Batch(1, 0, None, ascension=0)
        layout = dict(live.token_layout(*dimensions))
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError("invalid checkpoint architecture") from error
    saved_layout = dict(layout)
    potential = {}
    if legacy:
        saved_layout |= {"version": 59, "globals": 0}
        potential = {"potential_weights": config.pop("potential_weights"),
                     "potential_mode": config.pop("potential_mode", "health_multiplier")}
        config |= {"globals": layout["globals"], "value_heads": ["terminal_floor_scalar"]}
        if (checkpoint.get("winning_reservoir") or {}).get("rows"):
            raise ValueError("legacy winning reservoir rows lack run features")
    if (not legacy and checkpoint.get("feature_version") != FEATURE_VERSION) \
            or layout["version"] != FEATURE_VERSION:
        raise ValueError("incompatible feature version")
    if checkpoint["fingerprint"] != live.fingerprint() or checkpoint["layout"] != saved_layout:
        raise ValueError("checkpoint does not match simulator content")
    try:
        model = Agent(layout, *(config[key] for key in (
            "width", "layers", "heads", "feedforward", "head_width"
        )), pooling=config["pooling"], **potential).to(target)
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError("invalid checkpoint architecture") from error
    if config != architecture(model):
        raise ValueError("incompatible model architecture")
    state = checkpoint["model"]
    if legacy:
        state["potential.weight"] = model.potential.weight.detach().clone()
        groups = checkpoint["optimizer"]["param_groups"]
        parameter = sum(len(group["params"]) for group in groups)
        if len(groups) == 1:
            groups[0]["params"].append(parameter)
        else:
            groups.append(copy.deepcopy(groups[-1]) | {"params": [parameter]})
        checkpoint |= {"model_version": MODEL_VERSION, "feature_version": FEATURE_VERSION,
                       "layout": layout}
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
        struct.pack("<8I", model.width, model.layers, model.heads, model.feedforward,
                    len(DOMAINS), len(SEMANTIC_NAMES), model.concepts.num_embeddings, 1),
        struct.pack(f"<{2 * len(DOMAINS)}I", *(value for _name, codes, fields in DOMAINS
                                               for value in (codes, fields))),
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
        struct.pack(f"<{RUN_FEATURES}f", *model.potential.weight.reshape(-1).tolist()),
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
        if module.numeric is not None:
            add(module.numeric.weight)
        add_norm(module.norm)

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

    add(model.concepts.flattened()); add(model.concepts.amounts_flattened())
    for name, *_shape in DOMAINS:
        if name in model.encoders:
            add_encoder(model.encoders[name])
    for name in POOLING_DEFAULTS:
        add(model.summary_seed[name])
    for name in POOLS:
        add_linear(model.pool_projections[name])
    for index, (name, mode) in enumerate(model.pooling.items()):
        if (index < 8 and mode == "transformer") \
                or (index < 10 and mode.startswith("transformer")) \
                or (index == 10 and mode == "transformer"):
            add_transformer(model.pool_transformers[name])
    add_gru(model.move_gru)
    if model.continuation_gru is not None:
        add_gru(model.continuation_gru)
    add_norm(model.actor_norm); add_norm(model.action_norm)
    add_transformer(model.global_transformer); add_norm(model.global_norm)
    add_norm(model.graph_norm)
    for module in (model.graph_query, model.graph_key_value, model.graph_out):
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
