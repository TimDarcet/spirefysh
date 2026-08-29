import argparse
import copy
import fcntl
import hashlib
import json
import math
import multiprocessing
import os
import pickle
import struct
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from queue import Empty, Full

import numpy as np
import torch
from torch import nn

import sts2_sim


FEATURE_VERSION = 54
MODEL_VERSION = 61
PRECISIONS = ("fp32", "bf16")
WINNING_CAPACITY = 4096
CHANGE = "V61: add bounded public draw, map, and continuation position embeddings."
STAGES = [(0, bonus) for bonus in (24, 20, 16, 12, 8, 4, 0)] + [
    (ascension, 0) for ascension in range(1, 11)
]

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
    device float *output, device float *lse, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, token = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = token * 3 * width + head * dimension + column;
    if (end == begin + 1) {
        output[token * width + head * dimension + column] = qkv[query + 2 * width];
        if (column == 0) lse[token * heads + head] = 0.0f;
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
    output[token * width + head * dimension + column] = value / sum;
    if (column == 0) lse[token * heads + head] = maximum + log(sum);
}

kernel void backward_query(
    device const float *qkv, device const int *offsets, device const int *sequence,
    device const float *output, device const float *lse, device const float *grad_output,
    device float *grad_qkv, device float *delta, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, token = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = token * 3 * width + head * dimension + column;
    uint out = token * width + head * dimension + column;
    if (end == begin + 1) {
        grad_qkv[query] = 0.0f;
        if (column == 0) delta[token * heads + head] = 0.0f;
        return;
    }
    float q = qkv[query], grad = grad_output[out];
    float correction = head_sum(grad * output[out], dimension, lane), dq = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 3 * width + width + head * dimension;
        float key_value = qkv[k + column];
        float probability = exp(head_sum(q * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[token * heads + head]);
        float dp = head_sum(grad * qkv[k + width + column], dimension, lane);
        dq += probability * (dp - correction) * key_value * rsqrt(float(dimension));
    }
    grad_qkv[query] = dq;
    if (column == 0) delta[token * heads + head] = correction;
}

kernel void backward_key_value(
    device const float *qkv, device const int *offsets, device const int *sequence,
    device const float *output, device const float *lse, device const float *grad_output,
    device const float *delta, device float *grad_qkv, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, key = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[key], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, k = key * 3 * width + width + head * dimension;
    if (end == begin + 1) {
        grad_qkv[k + column] = 0.0f;
        grad_qkv[k + width + column] = grad_output[key * width + head * dimension + column];
        return;
    }
    float value = qkv[k + width + column];
    float dk = 0.0f, dv = 0.0f;
    for (int token = begin; token < end; ++token) {
        uint query = token * 3 * width + head * dimension + column;
        uint out = token * width + head * dimension + column;
        float q = qkv[query], key_value = qkv[k + column], grad = grad_output[out];
        float probability = exp(head_sum(q * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[token * heads + head]);
        float ds = probability * (head_sum(grad * value, dimension, lane) - delta[token * heads + head]);
        dk += ds * q * rsqrt(float(dimension));
        dv += probability * grad;
    }
    grad_qkv[k + column] = dk;
    grad_qkv[k + width + column] = dv;
}

kernel void attention_forward_bfloat(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device bfloat *output, device float *lse, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, token = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = token * 3 * width + head * dimension + column;
    if (end == begin + 1) {
        output[token * width + head * dimension + column] = qkv[query + 2 * width];
        if (column == 0) lse[token * heads + head] = 0.0f;
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
    output[token * width + head * dimension + column] = bfloat(value / sum);
    if (column == 0) lse[token * heads + head] = maximum + log(sum);
}

kernel void attention_backward_query_bfloat(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device const bfloat *output, device const float *lse, device const bfloat *grad_output,
    device bfloat *grad_qkv, device float *delta, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, token = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[token], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, query = token * 3 * width + head * dimension + column;
    uint out = token * width + head * dimension + column;
    if (end == begin + 1) {
        grad_qkv[query] = bfloat(0.0f);
        if (column == 0) delta[token * heads + head] = 0.0f;
        return;
    }
    float q = float(qkv[query]), grad = float(grad_output[out]);
    float correction = head_sum(grad * float(output[out]), dimension, lane), dq = 0.0f;
    for (int key = begin; key < end; ++key) {
        uint k = key * 3 * width + width + head * dimension;
        float key_value = float(qkv[k + column]);
        float probability = exp(head_sum(q * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[token * heads + head]);
        float dp = head_sum(grad * float(qkv[k + width + column]), dimension, lane);
        dq += probability * (dp - correction) * key_value * rsqrt(float(dimension));
    }
    grad_qkv[query] = bfloat(dq);
    if (column == 0) delta[token * heads + head] = correction;
}

kernel void attention_backward_key_value_bfloat(
    device const bfloat *qkv, device const int *offsets, device const int *sequence,
    device const bfloat *output, device const float *lse, device const bfloat *grad_output,
    device const float *delta, device bfloat *grad_qkv, constant uint& heads, constant uint& dimension,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint group = tid / SIMD_WIDTH, heads_per_group = SIMD_WIDTH / dimension;
    uint groups = heads / heads_per_group, key = group / groups;
    uint head = group % groups * heads_per_group + lane / dimension, column = lane % dimension;
    int row = sequence[key], begin = offsets[row], end = offsets[row + 1];
    uint width = heads * dimension, k = key * 3 * width + width + head * dimension;
    if (end == begin + 1) {
        grad_qkv[k + column] = bfloat(0.0f);
        grad_qkv[k + width + column] = grad_output[key * width + head * dimension + column];
        return;
    }
    float value = float(qkv[k + width + column]);
    float dk = 0.0f, dv = 0.0f;
    for (int token = begin; token < end; ++token) {
        uint query = token * 3 * width + head * dimension + column;
        uint out = token * width + head * dimension + column;
        float q = float(qkv[query]), key_value = float(qkv[k + column]), grad = float(grad_output[out]);
        float probability = exp(head_sum(q * key_value, dimension, lane) * rsqrt(float(dimension)) - lse[token * heads + head]);
        float ds = probability * (head_sum(grad * value, dimension, lane) - delta[token * heads + head]);
        dk += ds * q * rsqrt(float(dimension));
        dv += probability * grad;
    }
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

constant uint TOKEN_LANES = 32;
constant uint TOKEN_REDUCTION = 128;
constant uint NUMERIC_WIDTH = 24;

kernel void token_forward(
    device const int *indices, device const int *offsets, device const float *numeric,
    device const int *numeric_row, device const float *embedding,
    device const float *numeric_weight, device const float *gamma, device const float *beta,
    device float *input, device float *output, device float *stats,
    device int *occurrence, device int *count, constant uint& width,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint row = tid / TOKEN_LANES, base = row * width + lane;
    int begin = offsets[row], end = offsets[row + 1], numeric_index = numeric_row[row];
    float4 value = 0.0f;
    for (int position = begin; position < end; ++position) {
        uint source = indices[position] * width + lane;
        for (uint part = 0; part < width / TOKEN_LANES; ++part)
            value[part] += embedding[source + part * TOKEN_LANES];
    }
    if (numeric_index >= 0) {
        for (uint feature = 0; feature < NUMERIC_WIDTH; ++feature) {
            float number = numeric[numeric_index * NUMERIC_WIDTH + feature];
            for (uint part = 0; part < width / TOKEN_LANES; ++part)
                value[part] += number * numeric_weight[(lane + part * TOKEN_LANES) * NUMERIC_WIDTH + feature];
        }
    }
    float local_sum = 0.0f, local_square = 0.0f;
    for (uint part = 0; part < width / TOKEN_LANES; ++part) {
        local_sum += value[part]; local_square += value[part] * value[part];
    }
    float sum = simd_sum(local_sum), square = simd_sum(local_square);
    float mean = sum / width;
    float inverse_std = rsqrt(max(0.0f, square / width - mean * mean) + 1e-5f);
    for (uint part = 0; part < width / TOKEN_LANES; ++part) {
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
    constant uint& width,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint row = tid / TOKEN_LANES, base = row * width + lane;
    float mean = stats[row * 2], inverse_std = stats[row * 2 + 1];
    float4 normalized = 0.0f, grad = 0.0f;
    for (uint part = 0; part < width / TOKEN_LANES; ++part) {
        uint column = lane + part * TOKEN_LANES, target = base + part * TOKEN_LANES;
        normalized[part] = (input[target] - mean) * inverse_std;
        float activation = normalized[part] * gamma[column] + beta[column];
        grad[part] = activation > 0 ? grad_output[target] * gamma[column] : 0.0f;
    }
    float local_sum = 0.0f, local_product = 0.0f;
    for (uint part = 0; part < width / TOKEN_LANES; ++part) {
        local_sum += grad[part]; local_product += grad[part] * normalized[part];
    }
    float sum = simd_sum(local_sum), product = simd_sum(local_product);
    for (uint part = 0; part < width / TOKEN_LANES; ++part) {
        grad_input[base + part * TOKEN_LANES] = (
            grad[part] - sum / width - normalized[part] * product / width
        ) * inverse_std;
    }
}

kernel void token_backward_norm_partial(
    device const float *input, device const float *gamma, device const float *beta,
    device const float *stats, device const float *grad_output,
    device float *partial, constant uint& rows, constant uint& block_rows, constant uint& width,
    uint tid [[thread_position_in_grid]]) {
    uint block = tid / width, column = tid % width;
    uint begin = block * block_rows, end = min(rows, begin + block_rows);
    float dg = 0.0f, db = 0.0f;
    for (uint row = begin; row < end; ++row) {
        uint index = row * width + column;
        float normalized = (input[index] - stats[row * 2]) * stats[row * 2 + 1];
        float activation = normalized * gamma[column] + beta[column];
        float grad = activation > 0 ? grad_output[index] : 0.0f;
        dg += grad * normalized;
        db += grad;
    }
    partial[block * 2 * width + column] = dg;
    partial[(block * 2 + 1) * width + column] = db;
}

kernel void token_backward_norm_reduce(
    device const float *partial, device float *grad_gamma, device float *grad_beta,
    constant uint& blocks, constant uint& width,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_threadgroup]]) {
    threadgroup float gamma_sum[128], beta_sum[128];
    uint column = tid / TOKEN_REDUCTION;
    float dg = 0.0f, db = 0.0f;
    for (uint block = lane; block < blocks; block += TOKEN_REDUCTION) {
        dg += partial[block * 2 * width + column];
        db += partial[(block * 2 + 1) * width + column];
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
    half_library = None

    @staticmethod
    def forward(ctx, qkv, offsets, sequence, heads):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        if qkv.dtype == torch.float16 and _RaggedAttention.half_library is None:
            _RaggedAttention.half_library = torch.mps.compile_shader(
                _TRAINING_METAL.replace("bfloat", "half")
            )
        library = _RaggedAttention.half_library if qkv.dtype == torch.float16 else _RaggedAttention.library
        output = qkv.new_empty((len(qkv), qkv.shape[1] // 3))
        lse = torch.empty((len(qkv), heads), dtype=torch.float32, device=qkv.device)
        dimension = output.shape[1] // heads
        threads = len(qkv) * heads * dimension
        suffix = "_half" if qkv.dtype == torch.float16 else "_bfloat" if qkv.dtype == torch.bfloat16 else ""
        kernel = getattr(library, f"attention_forward{suffix}")
        kernel(
            qkv, offsets, sequence, output, lse, heads, dimension,
            threads=threads, group_size=32,
        )
        ctx.save_for_backward(qkv, offsets, sequence, output, lse)
        ctx.heads = heads
        ctx.dimension = dimension
        ctx.library, ctx.suffix = library, suffix
        return output

    @staticmethod
    def backward(ctx, grad_output):
        qkv, offsets, sequence, output, lse = ctx.saved_tensors
        grad_qkv = torch.empty_like(qkv)
        delta = torch.empty_like(lse)
        threads = len(qkv) * ctx.heads * ctx.dimension
        args = qkv, offsets, sequence, output, lse, grad_output.contiguous()
        query = getattr(ctx.library, f"attention_backward_query{ctx.suffix}") if ctx.suffix else ctx.library.backward_query
        key_value = getattr(ctx.library, f"attention_backward_key_value{ctx.suffix}") if ctx.suffix else ctx.library.backward_key_value
        query(
            *args, grad_qkv, delta, ctx.heads, ctx.dimension, threads=threads, group_size=32
        )
        key_value(
            *args, delta, grad_qkv, ctx.heads, ctx.dimension, threads=threads, group_size=32
        )
        return grad_qkv, None, None, None


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


class _FusedTokens(torch.autograd.Function):
    @staticmethod
    def forward(ctx, indices, offsets, numeric, numeric_index, numeric_row,
                embedding, numeric_weight, gamma, beta):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        rows = len(offsets) - 1
        width = embedding.shape[1]
        input = embedding.new_empty((rows, width))
        output = torch.empty_like(input)
        stats = embedding.new_empty((rows, 2))
        occurrence = torch.empty_like(indices)
        count = torch.empty(rows, dtype=torch.int32, device=embedding.device)
        _RaggedAttention.library.token_forward(
            indices, offsets, numeric, numeric_row, embedding, numeric_weight, gamma, beta,
            input, output, stats, occurrence, count, width, threads=rows * 32, group_size=32,
        )
        ctx.save_for_backward(
            indices, offsets, numeric, numeric_index, embedding, gamma, beta,
            input, stats, occurrence, count,
        )
        ctx.width = width
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
            ctx.width, threads=len(input) * 32, group_size=32,
        )
        blocks = (len(input) + 127) // 128
        partial = torch.empty((blocks, 2, ctx.width), dtype=torch.float32, device=input.device)
        _RaggedAttention.library.token_backward_norm_partial(
            input, gamma, beta, stats, grad_output, partial, len(input), 128, ctx.width,
            threads=blocks * ctx.width, group_size=ctx.width,
        )
        _RaggedAttention.library.token_backward_norm_reduce(
            partial, grad_gamma, grad_beta, blocks, ctx.width,
            threads=ctx.width * 128, group_size=128,
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


DOMAIN_SPECS = (
    ("run", 24, 16, 6, 36), ("phase", 16, 8, 12, 16), ("card", 20, 16, 73, 23),
    ("actor", 12, 16, 5, 27), ("power", 8, 4, 5, 8), ("history", 6, 28, 4, 32),
    ("status", 20, 8, 72, 17), ("relic", 12, 4, 8, 16), ("potion", 6, 2, 5, 6),
    ("orb", 6, 2, 5, 6), ("event", 4, 1, 4, 4), ("encounter", 4, 0, 3, 4),
    ("crystal", 8, 2, 5, 10), ("continuation", 24, 16, 80, 24),
    ("map_node", 12, 0, 7, 8), ("map_edge", 8, 0, 2, 4),
)
DOMAIN = {name: index for index, (name, *_shape) in enumerate(DOMAIN_SPECS)}
ACTION_FIELDS = (14, 12, 8, 20)
SEMANTIC_NAMES = (
    "character", "card", "power", "relic", "potion", "enemy", "enemy_move", "orb",
    "event", "encounter_normal", "encounter_elite", "encounter_boss", "enchantment",
    "card_type", "rarity", "room", "target", "pile", "phase", "action", "effect",
    "run_effect", "condition", "filter", "card_op", "scale", "actor_slot", "position",
    "flag_bit", "turn_flag_bit", "tag_bit", "run_kind", "phase_kind", "card_zone",
    "card_role", "order_kind", "actor_kind", "power_source", "history_kind", "status_kind",
    "relic_kind", "potion_kind", "orb_kind", "event_kind", "encounter_kind", "crystal_kind",
    "continuation_kind", "continuation_variant", "map_node_kind", "map_edge_kind",
    "route_kind", "requirement", "tinker_rider", "conveyor_dish", "boolean",
)


class SemanticEncoder(nn.Module):
    def __init__(self, semantic, numeric, width):
        super().__init__()
        self.field = nn.Parameter(torch.ones(semantic, width) + torch.randn(semantic, width) * .02)
        self.numeric = nn.Linear(numeric, width, bias=False)
        self.bias = nn.Parameter(torch.zeros(width))
        self.norm = nn.LayerNorm(width)

    def forward(self, semantic, numeric, embedding):
        if not len(semantic):
            return self.bias.new_empty((0, len(self.bias)))
        present = semantic != 0
        categorical = (embedding(semantic.long()) * self.field * present[:, :, None]).sum(1)
        categorical = categorical / present.sum(1).clamp_min(1).sqrt()[:, None]
        return torch.relu(self.norm(categorical + self.numeric(numeric) + self.bias))


class Agent(nn.Module):
    def __init__(self, layout, width=64, layers=2, heads=4, feedforward=128, head_width=None):
        super().__init__()
        self.layout = dict(layout)
        if self.layout["version"] != FEATURE_VERSION:
            raise ValueError("incompatible observation layout")
        expected = {name + suffix: value for name, unsigned, signed, semantic, numeric in DOMAIN_SPECS
                    for suffix, value in (("_u", unsigned), ("_s", signed),
                                          ("_c", semantic), ("_f", numeric))}
        expected |= {"domain_count": len(DOMAIN_SPECS), "action_u": ACTION_FIELDS[0],
                     "action_s": ACTION_FIELDS[1], "action_c": ACTION_FIELDS[2],
                     "action_f": ACTION_FIELDS[3], "card_zones": 13, "globals": 3605,
                     "card_known_position_semantic": 7,
                     "continuation_branch_semantic": 3, "continuation_path_semantic": 4,
                     "continuation_list_semantic": 5, "continuation_order_semantic": 6,
                     "map_node_floor_semantic": 2, "map_node_lane_semantic": 3}
        if any(self.layout.get(key) != value for key, value in expected.items()):
            raise ValueError("incompatible observation schema")
        ranges = sorted((self.layout[name + "_semantic_start"], self.layout[name + "_semantic_count"])
                        for name in SEMANTIC_NAMES)
        if (not ranges or ranges[0][0] != 1 or any(count < 1 for _start, count in ranges)
                or any(start + count != next_start
                       for (start, count), (next_start, _next_count) in zip(ranges, ranges[1:]))
                or ranges[-1][0] + ranges[-1][1] != self.layout.get("semantic_vocab")):
            raise ValueError("invalid semantic namespaces")
        if (width, layers, heads, feedforward) != (64, 2, 4, 128) or head_width not in (None, 112):
            raise ValueError("invalid architecture")
        self.width, self.layers, self.heads, self.feedforward = width, layers, heads, feedforward
        self.character_start = 0
        self.card_zones = 13
        self.entity_collections = 10
        self.state_width = self.layout["globals"] + (self.card_zones + self.entity_collections + 1) * width
        self.semantic = nn.Embedding(self.layout["semantic_vocab"], width, padding_idx=0)
        self.encoders = nn.ModuleDict({
            name: SemanticEncoder(semantic, numeric, width)
            for name, _unsigned, _signed, semantic, numeric in DOMAIN_SPECS
        })
        self.action_encoder = SemanticEncoder(ACTION_FIELDS[2], ACTION_FIELDS[3], width)
        self.card_state = nn.Parameter(torch.randn(self.card_zones, width) * .02)
        self.card_count = nn.Linear(6, width, bias=False)
        card_layer = nn.TransformerEncoderLayer(
            width, heads, feedforward, dropout=0, batch_first=True, norm_first=True, activation="relu"
        )
        self.card_transformer = nn.TransformerEncoder(card_layer, layers, enable_nested_tensor=False)
        self.child_tuple = nn.Linear(width, width)
        self.child_pool = nn.Linear(width + 2, width)
        self.child_state = nn.Parameter(torch.randn(8, width) * .02)
        self.actor_mlp = nn.Sequential(nn.Linear(9 * width, width), nn.LayerNorm(width), nn.ReLU())
        self.entity_tuple = nn.Linear(width, width)
        self.entity_pool = nn.Linear(width + 2, width)
        self.entity_state = nn.Parameter(torch.randn(self.entity_collections, width) * .02)
        self.graph_norm = nn.LayerNorm(width)
        self.graph_query = nn.Linear(width, width)
        self.graph_key_value = nn.Linear(width, 2 * width)
        self.graph_edge = nn.Linear(width, 2 * width, bias=False)
        self.graph_out = nn.Linear(width, width)
        self.graph_degree = nn.Linear(2, width, bias=False)
        self.graph_ff_norm = nn.LayerNorm(width)
        self.graph_ff = nn.Sequential(nn.Linear(width, feedforward), nn.ReLU(), nn.Linear(feedforward, width))
        self.graph_global = nn.Parameter(torch.randn(width) * .02)
        self.continuation_relation = nn.Linear(6, width, bias=False)
        self.continuation_tuple = nn.Linear(2 * width, width)
        self.continuation_pool = nn.Linear(width + 2, width)
        self.continuation_parent = nn.Linear(2 * width, width)
        self.continuation_norm = nn.LayerNorm(width)
        nn.init.normal_(self.graph_edge.weight, std=.01)
        nn.init.normal_(self.graph_out.weight, std=.01)
        nn.init.normal_(self.graph_degree.weight, std=.01)
        self.candidate_scale = nn.Parameter(torch.ones(len(DOMAIN_SPECS), width))
        self.candidate_bias = nn.Parameter(torch.zeros(len(DOMAIN_SPECS), width))
        self.candidate_combine = nn.Linear(2 * width + 2, width)
        self.path_adapter = nn.Linear(width, width, bias=False)
        self.target_adapter = nn.Linear(width, width, bias=False)
        self.action_legal = nn.Embedding(2, width)
        self.action_norm = nn.LayerNorm(width)
        self.head_width = head_width or 112
        self.policy = nn.Sequential(
            nn.Linear(self.state_width + width, self.head_width), nn.ReLU(), nn.Linear(self.head_width, 1),
        )
        self.value = nn.Sequential(
            nn.Linear(self.state_width, self.head_width), nn.ReLU(), nn.Linear(self.head_width, 1),
        )
        self.progress_value = nn.Sequential(
            nn.Linear(self.state_width, self.head_width), nn.ReLU(), nn.Linear(self.head_width, 1),
        )
        self.policy_entropy = None
        self._card_cache, self._graph_cache = {}, {}
        self.cache_stats = {"card_hit": 0, "card_miss": 0, "graph_hit": 0, "graph_miss": 0}
        nn.init.normal_(self.policy[-1].weight, std=0.01)
        nn.init.zeros_(self.policy[-1].bias)
        for head in (self.value, self.progress_value):
            nn.init.zeros_(head[-1].weight)
            nn.init.zeros_(head[-1].bias)

    def load_state_dict(self, state_dict, strict=True, assign=False):
        self._card_cache.clear(); self._graph_cache.clear()
        self.cache_stats = dict.fromkeys(self.cache_stats, 0)
        return super().load_state_dict(state_dict, strict, assign=assign)

    def train(self, mode=True):
        if mode:
            self._card_cache.clear(); self._graph_cache.clear()
            self.cache_stats = dict.fromkeys(self.cache_stats, 0)
        return super().train(mode)

    def encode_domains(self, domains):
        return tuple(self.encoders[name](*values[:2], self.semantic)[:values[2]]
                     for (name, *_), values in zip(DOMAIN_SPECS, domains))

    def encode_sequence(self, encoded, index, start, transformer, summary_only=False):
        offsets, destination, sequence, row, position, max_length, groups = index
        if encoded.device.type != "mps" or self.width // self.heads not in (16, 32):
            lengths = offsets[1:] - offsets[:-1]
            valid = torch.arange(max_length, device=encoded.device)[None] < lengths[:, None]
            current = encoded.new_zeros((len(lengths), max_length, self.width))
            current[:groups, 0] = start.to(current.dtype)
            if len(encoded):
                current[row, position] = encoded
            for layer in transformer.layers[:-1] if summary_only else transformer.layers:
                heads = layer.self_attn.num_heads
                qkv = nn.functional.linear(
                    layer.norm1(current), layer.self_attn.in_proj_weight,
                    layer.self_attn.in_proj_bias,
                ).reshape(len(lengths), max_length, 3, heads, self.width // heads)
                query, key, value = (part.transpose(1, 2) for part in qkv.unbind(2))
                attended = nn.functional.scaled_dot_product_attention(
                    query, key, value, attn_mask=valid[:, None, None],
                    dropout_p=layer.self_attn.dropout if self.training else 0,
                ).transpose(1, 2).reshape(len(lengths), max_length, self.width)
                current = current + layer.dropout1(layer.self_attn.out_proj(attended))
                current = current + layer.dropout2(layer.linear2(layer.dropout(
                    layer.activation(layer.linear1(layer.norm2(current)))
                )))
            if summary_only:
                layer = transformer.layers[-1]
                normalized = layer.norm1(current)
                weight, bias = layer.self_attn.in_proj_weight, layer.self_attn.in_proj_bias
                query = nn.functional.linear(
                    normalized[:, :1], weight[:self.width], bias[:self.width]
                ).reshape(len(lengths), 1, self.heads, -1).transpose(1, 2)
                key, value = nn.functional.linear(
                    normalized, weight[self.width:], bias[self.width:]
                ).reshape(len(lengths), max_length, 2, self.heads, -1).unbind(2)
                attended = nn.functional.scaled_dot_product_attention(
                    query, key.transpose(1, 2), value.transpose(1, 2),
                    attn_mask=valid[:, None, None], dropout_p=0,
                ).transpose(1, 2).reshape(len(lengths), 1, self.width)
                current = current[:, :1] + layer.dropout1(layer.self_attn.out_proj(attended))
                current = current + layer.dropout2(layer.linear2(layer.dropout(
                    layer.activation(layer.linear1(layer.norm2(current)))
                )))
            if transformer.norm is not None:
                current = transformer.norm(current)
            return current[:groups, 0], encoded.new_empty((0, self.width)) if summary_only else current[row, position]
        current = encoded.new_zeros((len(sequence), self.width))
        current[offsets[:groups]] = start.to(current.dtype)
        current[destination] = encoded
        for layer in transformer.layers[:-1] if summary_only else transformer.layers:
            qkv = nn.functional.linear(
                layer.norm1(current), layer.self_attn.in_proj_weight, layer.self_attn.in_proj_bias
            )
            attended = _RaggedAttention.apply(qkv, offsets, sequence, layer.self_attn.num_heads)
            current = current + layer.dropout1(layer.self_attn.out_proj(attended))
            current = current + layer.dropout2(layer.linear2(layer.dropout(
                layer.activation(layer.linear1(layer.norm2(current)))
            )))
        if summary_only:
            layer = transformer.layers[-1]
            normalized = layer.norm1(current)
            weight, bias = layer.self_attn.in_proj_weight, layer.self_attn.in_proj_bias
            starts = offsets[:-1]
            query = nn.functional.linear(normalized[starts], weight[:self.width], bias[:self.width])
            key_value = nn.functional.linear(normalized, weight[self.width:], bias[self.width:])
            attended = _RaggedSummaryAttention.apply(
                query, key_value, offsets, sequence, layer.self_attn.num_heads
            )
            current = current[starts] + layer.dropout1(layer.self_attn.out_proj(attended))
            current = current + layer.dropout2(layer.linear2(layer.dropout(
                layer.activation(layer.linear1(layer.norm2(current)))
            )))
        if transformer.norm is not None:
            current = transformer.norm(current)
        return (current[:groups], encoded.new_empty((0, self.width))) if summary_only else (
            current[offsets[:groups]], current[destination]
        )

    def pool(self, encoded, source, group, count, groups, state, tuple_linear, pool_linear):
        if not groups:
            return encoded.new_empty((0, self.width))
        pooled = encoded.new_zeros((groups, self.width))
        if len(source):
            values = encoded[source] if tuple_linear is None else torch.relu(tuple_linear(encoded[source]))
            pooled = pooled.index_add(0, group, values.to(pooled.dtype))
        pooled = pooled / count.clamp_min(1).sqrt()[:, None]
        exact = count[:, None].to(pooled.dtype)
        pooled = pool_linear(torch.cat((pooled, exact, exact.log1p()), 1))
        return torch.relu(pooled + state.to(pooled.dtype))

    def encode_cards(self, encoded, raw, cards):
        source, raw_source, sequence, zones, inverse, counts = cards
        states = self.card_state[zones] + self.card_count(counts.to(encoded.dtype))
        if self.training or encoded.device.type != "cpu":
            return self.encode_sequence(
                encoded[source], sequence, states, self.card_transformer, True,
            )[0][inverse]
        semantic, numeric = raw[-2:]
        lengths = (sequence[0][1:] - sequence[0][:-1] - 1).tolist()
        values = [None] * len(lengths)
        missing = []
        cursor = 0
        for group, length in enumerate(lengths):
            selected = source[cursor:cursor + length]
            raw_selected = raw_source[cursor:cursor + length]
            cursor += length
            key = (int(zones[group]), semantic[raw_selected].tobytes(), numeric[raw_selected].tobytes())
            value = self._card_cache.get(key)
            if value is None:
                self.cache_stats["card_miss"] += 1
                missing.append((group, key, selected))
            else:
                self.cache_stats["card_hit"] += 1
                values[group] = value
        if missing:
            selected = torch.cat([row[2] for row in missing])
            row = np.repeat(np.arange(len(missing), dtype=np.int32), [len(item[2]) for item in missing])
            summaries = self.encode_sequence(
                encoded[selected], sequence_index(row, len(missing), encoded.device),
                self.card_state[zones[[item[0] for item in missing]]]
                + self.card_count(counts[[item[0] for item in missing]].to(encoded.dtype)),
                self.card_transformer, True,
            )[0]
            for (group, key, _selected), value in zip(missing, summaries):
                value = value.detach(); values[group] = value; self._card_cache[key] = value
            while len(self._card_cache) > 8192:
                self._card_cache.pop(next(iter(self._card_cache)))
        return torch.stack(values)[inverse]

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
        offsets, group, position, maximum = index
        padded = score.new_full((len(offsets) - 1, maximum), -torch.inf)
        padded[group, position] = score.masked_fill(~legal, -torch.inf)
        normalizer = padded.logsumexp(1)
        value = score - torch.where(torch.isfinite(normalizer), normalizer, 0)[group]
        return torch.where(legal, value, torch.zeros_like(value))

    @staticmethod
    def group_entropy(log_probability, legal, index):
        offsets, group, _position, _maximum = index
        term = torch.where(legal, -log_probability.exp() * log_probability, 0)
        return term.new_zeros(len(offsets) - 1).index_add(0, group, term)

    def score(self, state, action, row):
        return self.policy(torch.cat((state[row], action), 1)).squeeze(-1)

    def decide(self, state, action, action_row, action_flat, actions, legal, sequence, temperature=1):
        scores = self.group_log_softmax(
            self.score(state, action, action_row) / temperature, legal, sequence,
        )
        policy = scores.new_zeros(len(state) * actions).scatter(
            0, action_flat, scores,
        ).reshape(len(state), actions)
        with torch.no_grad():
            entropy = self.group_entropy(scores, legal, sequence)
            legal_count = legal.new_zeros(len(state), dtype=torch.long).index_add(
                0, action_row, legal.long(),
            )
            self.policy_entropy = {
                "candidate": entropy.detach(), "normalized": torch.where(
                    legal_count > 1, entropy / legal_count.clamp_min(2).log(),
                    torch.zeros_like(entropy),
                ).detach(), "effective_actions": entropy.exp().detach(),
            }
        return policy

    def encode_map(self, encoded, index):
        node_source, edge_source, levels, global_sequence, current, entry, inverse, keys = index
        cached = [self._graph_cache.get(key) for key in keys]
        if not self.training and node_source.device.type == "cpu" and all(value is not None for value in cached):
            self.cache_stats["graph_hit"] += len(keys)
            nodes = torch.cat([value[0] for value in cached])
            global_map = torch.stack([value[1] for value in cached])
        else:
            if not self.training and node_source.device.type == "cpu":
                self.cache_stats["graph_miss"] += len(keys)
            nodes = encoded[DOMAIN["map_node"]][node_source]
            edges = encoded[DOMAIN["map_edge"]][edge_source]
            for parents, children, edge_rows, sequence, degree, count, _edge_count in levels:
                attended = self.attend(
                    self.graph_query(self.graph_norm(nodes[parents])),
                    self.graph_key_value(self.graph_norm(nodes[children]))
                    + self.graph_edge(edges[edge_rows]),
                    sequence,
                )
                features = torch.stack((degree / 8, degree.log1p() / 3), 1).to(nodes.dtype)
                updated = nodes[parents] + self.graph_out(attended) + self.graph_degree(features)
                updated = updated + self.graph_ff(self.graph_ff_norm(updated))
                nodes.index_copy_(0, parents[:count], updated[:count])
            normalized = self.graph_norm(nodes)
            query = self.graph_global.repeat(len(global_sequence[0]) - 1, 1)
            global_map = query + self.graph_out(self.attend(
                self.graph_query(query), self.graph_key_value(normalized), global_sequence,
            ))
            global_map = global_map + self.graph_ff(self.graph_ff_norm(global_map))
            if not self.training and node_source.device.type == "cpu":
                offsets = global_sequence[0].tolist()
                for index, key in enumerate(keys):
                    self._graph_cache[key] = (
                        nodes[offsets[index]:offsets[index + 1]].detach(), global_map[index].detach(),
                    )
                while len(self._graph_cache) > 8192:
                    self._graph_cache.pop(next(iter(self._graph_cache)))
        position = torch.where(current >= 0, current, entry)
        return nodes[position], global_map[inverse], nodes

    def encode_continuations(self, encoded, index):
        source, levels = index
        values = encoded[source]
        for parents, children, group, count, relation in levels:
            pooled = values.new_zeros((len(parents), self.width))
            if len(children):
                related = self.continuation_relation(relation.to(values.dtype))
                messages = torch.relu(self.continuation_tuple(torch.cat((values[children], related), 1)))
                pooled = pooled.index_add(0, group, messages.to(values.dtype))
            pooled = pooled / count.clamp_min(1).sqrt()[:, None]
            exact = count[:, None].to(values.dtype)
            pooled = self.continuation_pool(torch.cat((pooled, exact, exact.log1p()), 1))
            parent = self.continuation_parent(torch.cat((values[parents], pooled), 1))
            values = values.index_copy(
                0, parents, torch.relu(self.continuation_norm(parent)).to(values.dtype),
            )
        return values

    def encode_state(self, globals_, domains, encoded, index):
        cards, actors, entities, continuation_index, continuation_roots, continuation_rows, map_ = index
        card = self.encode_cards(encoded[DOMAIN["card"]], domains[DOMAIN["card"]], cards)
        base_source, actor_rows, children = actors
        child_encoded = {
            domain: torch.relu(self.child_tuple(encoded[domain]))
            for domain in {domain for domain, *_ in children}
        } if self.training else {}
        child = [self.pool(
            child_encoded.get(domain, encoded[domain]), source, group, count, len(base_source),
            self.child_state[family], None if self.training else self.child_tuple, self.child_pool,
        ) for family, (domain, source, group, count) in enumerate(children)]
        actor = (
            self.actor_mlp(torch.cat((encoded[DOMAIN["actor"]][base_source], *child), 1))
            if len(base_source) else encoded[DOMAIN["actor"]].new_empty((0, self.width))
        )
        continuation = self.encode_continuations(
            encoded[DOMAIN["continuation"]], continuation_index,
        )
        _position, global_map, nodes = self.encode_map(encoded, map_)
        batch = len(globals_)
        summaries = []
        for collection, (domain, source, group, count) in enumerate(entities[:2]):
            summaries.append(self.pool(
                encoded[domain], source, group, count, batch,
                self.entity_state[collection], self.entity_tuple, self.entity_pool,
            ))
        summaries.append(self.pool(
            actor, torch.arange(len(actor), device=actor.device), actor_rows,
            torch.bincount(actor_rows, minlength=batch).to(actor.dtype), batch,
            self.entity_state[2], self.entity_tuple, self.entity_pool,
        ))
        for collection, (domain, source, group, count) in enumerate(entities[2:], 3):
            summaries.append(self.pool(
                encoded[domain], source, group, count, batch,
                self.entity_state[collection], self.entity_tuple, self.entity_pool,
            ))
        summaries.append(self.pool(
            continuation, continuation_roots, continuation_rows,
            torch.bincount(continuation_rows, minlength=batch).to(continuation.dtype), batch,
            self.entity_state[-1], self.entity_tuple, self.entity_pool,
        ))
        state = torch.cat((globals_.float(), card.reshape(batch, -1), *summaries, global_map), 1)
        if state.shape[1] != self.state_width:
            raise ValueError("invalid state width")
        return state, nodes, actor, continuation

    def encode_actions(self, encoded, continuation, actor, values, index, nodes):
        semantic, numeric = values
        action_row, action_flat, legal, sequence, actions, sources, path, target = index
        base = self.action_encoder(semantic, numeric, self.semantic)[:len(action_row)]
        pooled = base.new_zeros(base.shape); count = base.new_ones(len(base))
        for domain, (source, group, domain_count, inverse, groups) in enumerate(sources):
            summed = pooled.new_zeros((groups, self.width))
            if len(source):
                values = continuation[source] if domain == DOMAIN["continuation"] else encoded[domain][source]
                summed = summed.index_add(0, group, values.to(summed.dtype))
            pooled = pooled + (
                summed * self.candidate_scale[domain] + domain_count[:, None] * self.candidate_bias[domain]
            )[inverse]
            count = count + domain_count[inverse]
        for index, values, adapter in ((path, nodes, self.path_adapter),):
            present = index >= 0
            if present.any():
                selected = present.nonzero().squeeze(1)
                pooled = pooled.index_add(0, selected, adapter(values[index[present]]).to(pooled.dtype))
                count = count + present.to(count.dtype)
        present = target >= 0
        if present.any():
            selected = present.nonzero().squeeze(1)
            pooled = pooled.index_add(0, selected, self.target_adapter(actor[target[present]]).to(pooled.dtype))
            count = count + present.to(count.dtype)
        exact = count[:, None].to(pooled.dtype)
        action = self.candidate_combine(torch.cat((
            base + self.action_legal(legal.long()), pooled / count.clamp_min(1).sqrt()[:, None],
            exact, exact.log1p(),
        ), 1))
        return torch.relu(self.action_norm(action)), action_row, action_flat, actions, legal, sequence

    def forward(self, _character, globals_, domains, state_index, action_values, action_index,
                return_state=False, policy_only=False, temperature=1):
        encoded = self.encode_domains(domains)
        state, nodes, actor, continuation = self.encode_state(globals_, domains, encoded, state_index)
        action, action_row, action_flat, actions, legal, sequence = self.encode_actions(
            encoded, continuation, actor, action_values, action_index, nodes,
        )
        policy = self.decide(state, action, action_row, action_flat, actions, legal, sequence, temperature)
        if policy_only:
            return policy
        output = (
            policy, self.value(state).squeeze(-1), self.progress_value(state).squeeze(-1),
        )
        return (*output, state) if return_state else output


def architecture(model):
    return {
        "width": model.width, "layers": model.layers, "heads": model.heads,
        "feedforward": model.feedforward, "head_width": model.head_width,
        "domains": [name for name, *_ in DOMAIN_SPECS], "card_zones": model.card_zones,
        "globals": model.layout["globals"], "entity_collections": model.entity_collections,
        "state_width": model.state_width, "policy_input": model.state_width + model.width,
        "map_layers": 1, "fusion_layers": 0,
        "final_attention": "query_only", "actor_hierarchy": "DeepSets",
        "tuple_pool": "sum/sqrt(count)+count+log1p(count)",
        "candidate_attention": "none; direct state+candidate scorer",
        "policy_factorization": "one shared direct candidate scorer",
        "menu_invariance": "target-cardinality and candidate-permutation invariant",
        "policy_entropy": ["candidate", "normalized", "effective_actions"],
        "map": "reverse-topological sparse edge-conditioned attention",
        "integer_encoding": "namespaced semantic IDs + field-specific float32 auxiliaries",
        "continuations": "bottom-up parent/branch/path relational pooling",
        "candidate_context": "complete public payload, Path context, and contextual target actor",
        "semantic_vocab": model.semantic.num_embeddings, "candidate_numeric": ACTION_FIELDS[3],
        "value_heads": ["win_logit", "progress"],
        "winning_reservoir": WINNING_CAPACITY,
    }


def predict(model, inputs, precision, temperature=1, policy_only=False):
    if precision not in PRECISIONS:
        raise ValueError("precision must be fp32 or bf16")
    kind = inputs[0].device.type
    if precision != "fp32" and kind not in ("mps", "cuda"):
        raise ValueError(f"{precision} requires MPS or CUDA")
    dtype = torch.bfloat16 if precision == "bf16" else torch.float16
    with torch.autocast(kind, dtype=dtype, enabled=precision != "fp32"):
        output = model(*inputs[:6], temperature=temperature, policy_only=policy_only)
    return output.float() if policy_only else tuple(value.float() for value in output)


def trust_region_step(model, optimizer, inputs, action, old, fresh, denominator, precision, limit,
                      temperature=1, attempts=3):
    nn.utils.clip_grad_norm_(model.parameters(), .5)
    parameters = list(model.parameters())
    state = [value for values in optimizer.state.values() for value in values.values()
             if torch.is_tensor(value)]
    optimizer_state = copy.deepcopy(optimizer.state_dict()) if any(
        parameter.grad is not None and parameter not in optimizer.state for parameter in parameters
    ) else None
    saved = getattr(optimizer, "_trust_region_saved", None)
    if saved is None or len(saved[1]) != len(state):
        saved = ([value.detach().clone() for value in parameters],
                 [value.detach().clone() for value in state])
        optimizer._trust_region_saved = saved
    else:
        torch._foreach_copy_(saved[0], parameters)
        for dtype in {value.dtype for value in state}:
            selected = [index for index, value in enumerate(state) if value.dtype == dtype]
            torch._foreach_copy_([saved[1][index] for index in selected],
                                 [state[index] for index in selected])
    lrs = [group["lr"] for group in optimizer.param_groups]

    def restore():
        with torch.no_grad():
            torch._foreach_copy_(parameters, saved[0])
        if optimizer_state is not None:
            optimizer.load_state_dict(optimizer_state)
        else:
            with torch.no_grad():
                for dtype in {value.dtype for value in state}:
                    selected = [index for index, value in enumerate(state) if value.dtype == dtype]
                    torch._foreach_copy_([state[index] for index in selected],
                                         [saved[1][index] for index in selected])

    proposals = []
    try:
        for attempt in range(attempts):
            if attempt:
                restore()
            for group, lr in zip(optimizer.param_groups, lrs):
                group["lr"] = lr * .25 ** attempt
            optimizer.step()
            with torch.no_grad():
                kind = inputs[0].device.type
                dtype = torch.bfloat16 if precision == "bf16" else torch.float16
                with torch.autocast(kind, dtype=dtype, enabled=precision != "fp32"):
                    logits = model(
                        *inputs[:6], policy_only=True, temperature=temperature,
                    ).float()[:len(action)]
                logits = logits.masked_fill(~inputs[6][:len(action)], -torch.inf)
                log_ratio = logits.log_softmax(-1).gather(1, action[:, None]).squeeze(1) - old
                post_kl = (((log_ratio.exp() - 1 - log_ratio) * fresh).sum() / denominator).float()
            proposals.append(float(post_kl))
            if torch.isfinite(post_kl) and proposals[-1] <= limit:
                return True, proposals
        restore()
        return False, proposals
    except Exception:
        restore()
        raise
    finally:
        for group, lr in zip(optimizer.param_groups, lrs):
            group["lr"] = lr


def critic_only_step(model, optimizer, parameters=None):
    parameters = parameters or (tuple(model.value.parameters()) + tuple(model.progress_value.parameters()))
    selected = {id(parameter) for parameter in parameters}
    for parameter in model.parameters():
        if id(parameter) not in selected:
            parameter.grad = None
    nn.utils.clip_grad_norm_(parameters, .5)
    optimizer.step()


def sequence_index(row, batch, target, pad=False):
    counts = np.bincount(row, minlength=batch).astype(np.int32)
    lengths = counts + 1
    starts = np.repeat(np.cumsum(counts) - counts, counts)
    position = np.arange(len(row), dtype=np.int32) - starts + 1
    destination = np.repeat(np.cumsum(lengths) - lengths, counts) + position
    if pad:
        padded_batch = (batch // 256 + 1) * 256
        lengths = np.pad(lengths, (0, padded_batch - batch), constant_values=1)
        extra = (-int(lengths.sum())) % 4096
        dummy = padded_batch - batch
        lengths[batch:] += extra // dummy
        lengths[batch:batch + extra % dummy] += 1
    offsets = np.r_[0, np.cumsum(lengths, dtype=np.int32)].astype(np.int32)
    sequence = np.repeat(np.arange(len(lengths), dtype=np.int32), lengths)
    return tuple(torch.as_tensor(value, device=target) for value in (
        offsets, destination, sequence, row, position,
    )) + (int(lengths.max()), batch)


def candidate_index(row, batch, target, allow_empty=False):
    row = np.asarray(row, np.int32)
    lengths = np.bincount(row, minlength=batch).astype(np.int32)
    if not allow_empty and not np.all(lengths):
        raise ValueError("state without represented candidate")
    offsets = np.r_[0, np.cumsum(lengths, dtype=np.int32)].astype(np.int32)
    starts = np.repeat(offsets[:-1], lengths)
    position = np.arange(len(row), dtype=np.int32) - starts
    return tuple(torch.as_tensor(value, device=target) for value in (
        offsets, row, position,
    )) + (int(lengths.max()),)


def observation_legal(observation):
    return np.asarray(observation[3][-1], bool)


def flat_domains(observation):
    batch = len(observation[0]); legal = observation_legal(observation)
    flat = len(observation[3]) == 7
    represented = None if flat else np.asarray(observation[3][4], bool)
    if not flat and np.any(~represented.any(1)):
        raise ValueError("state without represented candidate")
    if flat:
        action_row, action_position = map(lambda value: np.asarray(value, np.int32), observation[3][4:6])
    else:
        action_row, action_position = np.nonzero(represented)
        lookup = np.full(represented.shape, -1, np.int32)
        lookup[action_row, action_position] = np.arange(len(action_row), dtype=np.int32)
    domains = []
    for (name, unsigned, signed, semantic, numeric), values in zip(DOMAIN_SPECS, observation[2]):
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
    pad_sequences = model.training and model.card_state.device.type == "mps"
    tensor = lambda value: torch.as_tensor(value, dtype=torch.long, device=target)
    domain_tensors = []
    domain_inverse = []
    for (name, *_), (_u, _s, semantic, numeric, _row, _scope) in zip(DOMAIN_SPECS, domains):
        if semantic.max(initial=0) >= model.semantic.num_embeddings:
            raise ValueError(f"invalid {name} semantic id")
        key = np.ascontiguousarray(np.concatenate((semantic, numeric.view(np.uint32)), 1))
        if len(key):
            first, inverse = map(np.asarray, sts2_sim.unique_rows(key))
        else:
            first = inverse = np.empty(0, np.intp)
        count = len(first)
        size = 1 << (count - 1).bit_length() if model.training and count else count
        domain_tensors.append((
            torch.as_tensor(np.pad(semantic[first], ((0, size - count), (0, 0))).astype(np.int64),
                            device=target),
            torch.as_tensor(np.pad(numeric[first], ((0, size - count), (0, 0))), device=target),
            count, semantic, numeric,
        ))
        domain_inverse.append(inverse)
    domain_tensors = tuple(domain_tensors)
    state_source = [np.flatnonzero(scope < 0).astype(np.int32)
                    for _u, _s, _c, _f, _row, scope in domains]
    card_u, _card_s, _card_c, _card_f, card_row, _card_scope = domains[DOMAIN["card"]]
    card_source = state_source[DOMAIN["card"]]
    card_group = card_row * model.card_zones + card_u[:, 0].astype(np.int32)
    if len(card_source) and (card_group[card_source].min() < 0 or card_group[card_source].max() >= batch * model.card_zones):
        raise ValueError("invalid card zone")
    card_source = ordered(card_source, card_group, card_u[:, 2], card_u[:, 3])
    card_group = card_group[card_source]
    card_counts = np.bincount(card_group, minlength=batch * model.card_zones)
    card_top = np.bincount(card_group, card_u[card_source, 2] == 1,
                           minlength=batch * model.card_zones)
    card_bottom = np.bincount(card_group, card_u[card_source, 2] == 2,
                              minlength=batch * model.card_zones)
    positions = np.arange(len(card_group), dtype=np.int32) - np.repeat(
        np.cumsum(card_counts) - card_counts, card_counts
    )
    card_keys = np.full(
        (batch * model.card_zones, card_counts.max(initial=0) + 1),
        np.iinfo(np.uint32).max, np.uint32,
    )
    card_keys[:, 0] = np.arange(batch * model.card_zones) % model.card_zones
    card_keys[card_group, positions + 1] = domain_inverse[DOMAIN["card"]][card_source]
    empty = np.full((model.card_zones, card_keys.shape[1]), np.iinfo(np.uint32).max, np.uint32)
    empty[:, 0] = np.arange(model.card_zones)
    first, card_inverse = map(np.asarray, sts2_sim.unique_rows(np.concatenate((empty, card_keys))))
    card_inverse = card_inverse[model.card_zones:].astype(np.int32)
    representatives = first[first >= model.card_zones].astype(np.int32) - model.card_zones
    keep = np.zeros(batch * model.card_zones, bool); keep[representatives] = True
    card_source, card_group = card_source[keep[card_group]], card_inverse[card_group[keep[card_group]]]
    card_zones = np.r_[np.arange(model.card_zones),
                       np.arange(batch * model.card_zones) % model.card_zones][first]
    card_counts = np.stack((card_counts, np.log1p(card_counts), card_top, np.log1p(card_top),
                            card_bottom, np.log1p(card_bottom)), 1).astype(np.float32)
    card_counts = np.concatenate((np.zeros((model.card_zones, 6), np.float32), card_counts))[first]
    card_groups = len(first)
    actor_u, _actor_s, _actor_c, _actor_f, actor_rows, _actor_scope = domains[DOMAIN["actor"]]
    base_source = ordered(state_source[DOMAIN["actor"]], actor_rows, actor_u[:, 0])
    owners = [(int(actor_rows[source]), int(actor_u[source, 0])) for source in base_source]
    if len(owners) != len(set(owners)):
        raise ValueError("duplicate actor")
    actor_lookup = {owner: index for index, owner in enumerate(owners)}
    children = []
    for domain, kind in ((DOMAIN["power"], None), (DOMAIN["history"], None),
                         *((DOMAIN["status"], value) for value in range(6))):
        u, _s, _c, _f, row, scope = domains[domain]
        source = state_source[domain]
        if kind is not None:
            source = source[u[source, 1] == kind]
        group = np.asarray([actor_lookup.get((int(row[i]), int(u[i, 0])), -1) for i in source], np.int32)
        if np.any(group < 0):
            raise ValueError("actor child without actor")
        order = np.argsort(group, kind="stable")
        source, group = source[order], group[order]
        children.append((domain, tensor(domain_inverse[domain][source]), tensor(group), torch.as_tensor(
            np.bincount(group, minlength=len(base_source)).astype(np.float32), device=target,
        )))
    entities = []
    for name in ("run", "phase", "relic", "potion", "orb", "event", "encounter", "crystal"):
        domain = DOMAIN[name]; _u, _s, _c, _f, row, _scope = domains[domain]
        source = ordered(state_source[domain], row)
        group = row[source].astype(np.int32)
        entities.append((
            domain, tensor(domain_inverse[domain][source]), tensor(group),
            torch.as_tensor(np.bincount(group, minlength=batch).astype(np.float32), device=target),
        ))
    cont_u, _cont_s, _cont_c, _cont_f, cont_row, cont_scope = domains[DOMAIN["continuation"]]
    context = np.where(cont_scope < 0, cont_row, batch + cont_scope).astype(np.int64)
    cont_source = ordered(np.arange(len(cont_u), dtype=np.int32), context, cont_u[:, 5], cont_u[:, 2])
    cont_position = np.full(len(cont_u), -1, np.int32); cont_position[cont_source] = np.arange(len(cont_source))
    frame = {(int(context[source]), int(cont_u[source, 2])): int(cont_position[source])
             for source in cont_source}
    if len(frame) != len(cont_source):
        raise ValueError("duplicate continuation frame")
    no_node = np.iinfo(np.uint32).max
    parent = np.full(len(cont_source), -1, np.int32)
    for position, source in enumerate(cont_source):
        if cont_u[source, 3] != no_node:
            parent[position] = frame.get((int(context[source]), int(cont_u[source, 3])), -1)
            if parent[position] < 0 or cont_u[cont_source[parent[position]], 5] >= cont_u[source, 5]:
                parent_path = None if parent[position] < 0 else int(
                    cont_u[cont_source[parent[position]], 5]
                )
                raise ValueError(
                    f"invalid continuation parent row={int(cont_row[source])} "
                    f"scope={int(cont_scope[source])} context={int(context[source])} "
                    f"frame={int(cont_u[source, 2])} parent={int(cont_u[source, 3])} "
                    f"path={int(cont_u[source, 5])} parent_path={parent_path}"
                )
    child_counts = np.bincount(parent[parent >= 0], minlength=len(cont_source))
    if np.any(child_counts != cont_u[cont_source, 9]):
        raise ValueError("invalid continuation arity")
    continuation_levels = []
    for depth in sorted(np.unique(cont_u[cont_source, 5]), reverse=True):
        parents = np.flatnonzero(cont_u[cont_source, 5] == depth)
        lookup = np.full(len(cont_source), -1, np.int32); lookup[parents] = np.arange(len(parents))
        selected = np.flatnonzero((parent >= 0) & (lookup[parent] >= 0))
        group = lookup[parent[selected]]
        relation = domains[DOMAIN["continuation"]][3][cont_source[selected], 16:22]
        continuation_levels.append((
            tensor(parents), tensor(selected), tensor(group),
            torch.as_tensor(np.bincount(group, minlength=len(parents)).astype(np.float32), device=target),
            torch.as_tensor(relation, device=target),
        ))
    roots = np.flatnonzero(parent < 0)
    continuation_index = (tensor(domain_inverse[DOMAIN["continuation"]][cont_source]),
                          tuple(continuation_levels))
    continuation_roots = tensor(roots[cont_scope[cont_source[roots]] < 0])
    continuation_rows = cont_row[cont_source[roots]][cont_scope[cont_source[roots]] < 0]
    node_u, _node_s, node_c, node_f, node_row, _node_scope = domains[DOMAIN["map_node"]]
    edge_u, _edge_s, edge_c, edge_f, edge_row, _edge_scope = domains[DOMAIN["map_edge"]]
    node_source = ordered(state_source[DOMAIN["map_node"]], node_row, node_u[:, 0])
    edge_source = ordered(state_source[DOMAIN["map_edge"]], edge_row, edge_u[:, 0], edge_u[:, 1])
    node_offsets = np.r_[0, np.cumsum(np.bincount(node_row[node_source], minlength=batch))]
    edge_offsets = np.r_[0, np.cumsum(np.bincount(edge_row[edge_source], minlength=batch))]
    node_groups = [node_source[node_offsets[row]:node_offsets[row + 1]] for row in range(batch)]
    edge_groups = [edge_source[edge_offsets[row]:edge_offsets[row + 1]] for row in range(batch)]
    graph_keys = tuple((node_u[nodes].tobytes(), node_c[nodes].tobytes(), node_f[nodes].tobytes(),
                        edge_u[edges].tobytes(), edge_c[edges].tobytes(), edge_f[edges].tobytes())
                       for nodes, edges in zip(node_groups, edge_groups))
    unique = {}
    representatives = []
    inverse = np.empty(batch, np.int32)
    for row, key in enumerate(graph_keys):
        if key not in unique:
            unique[key] = len(representatives)
            representatives.append(row)
        inverse[row] = unique[key]
    node_source = np.concatenate([node_groups[row] for row in representatives])
    edge_source = np.concatenate([edge_groups[row] for row in representatives])
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
        order = np.argsort(edge_src[selected], kind="stable"); selected = selected[order]
        parents, group = np.unique(edge_src[selected], return_inverse=True)
        levels.append((parents, edge_dst[selected], selected, group,
                       node_u[node_source[parents], 9].astype(np.float32)))
    if model.training and levels:
        bucket = lambda count: min(
            size for power in range(max(1, (count - 1).bit_length()), 32)
            for size in (3 * (1 << power) // 4, 1 << power) if size >= count
        )
        padded = []
        for parents, destinations, edge_rows, group, degree in levels:
            count = len(parents)
            edge_count = len(destinations)
            edge_size = bucket(edge_count)
            parent_size = bucket(count + (edge_size > edge_count))
            extra = edge_size - len(destinations)
            padded.append((
                tensor(np.pad(parents, (0, parent_size - count))),
                tensor(np.pad(destinations, (0, extra))), tensor(np.pad(edge_rows, (0, extra))),
                candidate_index(np.r_[group, np.full(extra, count, np.int32)],
                                parent_size, target, True),
                torch.as_tensor(np.pad(degree, (0, parent_size - count)), device=target),
                count, edge_count,
            ))
        levels = padded
    else:
        levels = [(tensor(parents), tensor(destinations), tensor(edge_rows),
                   candidate_index(group, len(parents), target),
                   torch.as_tensor(degree, device=target), len(parents), len(destinations))
                  for parents, destinations, edge_rows, group, degree in levels]
    node_rows = np.repeat(np.arange(len(representatives), dtype=np.int32),
                          [len(node_groups[row]) for row in representatives])
    global_sequence = candidate_index(node_rows, len(representatives), target)
    run_u, _run_s, _run_c, _run_f, run_row, _run_scope = domains[DOMAIN["run"]]
    run_source = state_source[DOMAIN["run"]]
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
    map_index = (tensor(domain_inverse[DOMAIN["map_node"]][node_source]),
                 tensor(domain_inverse[DOMAIN["map_edge"]][edge_source]), tuple(levels), global_sequence,
                 tensor(current), tensor(entry), tensor(inverse),
                 tuple(graph_keys[row] for row in representatives))
    actor_rows = actor_rows[base_source].astype(np.int32)
    state_index = (
        (tensor(domain_inverse[DOMAIN["card"]][card_source]), tensor(card_source),
         sequence_index(card_group, card_groups, target, pad_sequences), tensor(card_zones),
         tensor(card_inverse), torch.as_tensor(card_counts, device=target)),
        (tensor(domain_inverse[DOMAIN["actor"]][base_source]), tensor(actor_rows), tuple(children)),
        tuple(entities), continuation_index, continuation_roots, tensor(continuation_rows), map_index,
    )
    candidate_sources = []
    for domain, (u, s, _c, _f, _row, scope) in enumerate(domains):
        if domain == DOMAIN["continuation"]:
            selected = roots[cont_scope[cont_source[roots]] >= 0]
            group = cont_scope[cont_source[selected]]
            candidate_sources.append((
                tensor(selected), tensor(group),
                torch.as_tensor(np.bincount(group, minlength=len(action_row)).astype(np.float32),
                                device=target),
                tensor(np.arange(len(action_row), dtype=np.int32)), len(action_row),
            ))
            continue
        source = np.flatnonzero(scope >= 0).astype(np.int32)
        group = scope[source]
        order = np.argsort(group, kind="stable"); source, group = source[order], group[order]
        counts = np.bincount(group, minlength=len(action_row))
        if counts.max(initial=0) <= 1:
            unique_source, inverse_source = np.unique(
                domain_inverse[domain][source], return_inverse=True
            )
            inverse = np.zeros(len(action_row), np.int32)
            inverse[group] = inverse_source + 1
            candidate_sources.append((
                tensor(unique_source), tensor(np.arange(1, len(unique_source) + 1)),
                torch.as_tensor(np.r_[0, np.ones(len(unique_source), np.float32)], device=target),
                tensor(inverse), len(unique_source) + 1,
            ))
            continue
        positions = np.arange(len(group), dtype=np.int32) - np.repeat(np.cumsum(counts) - counts, counts)
        keys = np.full((len(action_row), counts.max()), np.iinfo(np.uint32).max, np.uint32)
        keys[group, positions] = domain_inverse[domain][source]
        keys.sort(1)
        empty = np.full((1, keys.shape[1]), np.iinfo(np.uint32).max, np.uint32)
        first, inverse = map(np.asarray, sts2_sim.unique_rows(np.vstack((empty, keys))))
        inverse = inverse[1:].astype(np.int32)
        representatives = first[first > 0].astype(np.int32) - 1
        keep = np.zeros(len(action_row), bool); keep[representatives] = True
        source, group = source[keep[group]], inverse[group[keep[group]]]
        candidate_sources.append((
            tensor(domain_inverse[domain][source]), tensor(group),
            torch.as_tensor(np.bincount(group, minlength=len(first)).astype(np.float32),
                            device=target),
            tensor(inverse), len(first),
        ))
    action_u, action_s, action_c, action_f = action
    action_count = len(action_u)
    action_size = 1 << (action_count - 1).bit_length() if model.training and action_count else action_count
    if action_c.max(initial=0) >= model.semantic.num_embeddings:
        raise ValueError("invalid action semantic id")
    action_c = np.pad(action_c, ((0, action_size - action_count), (0, 0)))
    action_f = np.pad(action_f, ((0, action_size - action_count), (0, 0)))
    action_legal = legal[action_row, action_position]
    path = np.full(len(action_row), -1, np.int32)
    path_id = action_u[:, 4]
    present = path_id != np.iinfo(np.uint32).max
    if path_id[present].max(initial=0) > max_node:
        raise ValueError("path candidate references missing node")
    path[present] = node_lookup[action_row[present], path_id[present].astype(np.intp)]
    if np.any(path[present] < 0):
        raise ValueError("path candidate references missing node")
    actor_target = np.full(len(action_row), -1, np.int32)
    owner = action_u[:, 1]
    present = (owner != 0) & (owner != np.iinfo(np.uint32).max)
    for index in np.flatnonzero(present):
        actor_target[index] = actor_lookup.get((int(action_row[index]), int(owner[index])), -1)
    if np.any(actor_target[present] < 0):
        raise ValueError("candidate target references missing actor")
    action_index = tuple(torch.as_tensor(value, device=target) for value in (
        action_row, action_row * actions + action_position, action_legal,
    )) + (candidate_index(action_row, batch, target), actions, tuple(candidate_sources),
          tensor(path), tensor(actor_target))
    return (
        torch.as_tensor(character, dtype=torch.long, device=target),
        torch.as_tensor(globals_, device=target), domain_tensors, state_index,
        tuple(torch.as_tensor(value, device=target) for value in (
            action_c.astype(np.int64), action_f.astype(np.float32),
        )), action_index, torch.as_tensor(legal, device=target),
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
    if len(row) != 6:
        raise ValueError("packed observation lacks its Rust digest")
    character, globals_, counts, exact, actions, digest = row
    globals_, counts, exact, actions = map(np.asarray, (globals_, counts, exact, actions))
    if globals_.dtype != np.float32 or globals_.shape != (3605,) or not np.isfinite(globals_).all() \
            or counts.dtype != np.uint32 or counts.ndim != 1 or exact.dtype != np.uint32 or exact.ndim != 1 \
            or actions.dtype != np.uint32 or actions.ndim != 2:
        raise ValueError("invalid packed observation arrays")
    character, digest = int(character), int(digest)
    if character == np.iinfo(np.uint8).max:
        if (digest or globals_.any() or counts.shape != (len(DOMAIN_SPECS),) or counts.any() or len(exact)
                or actions.shape != (0, sum(ACTION_FIELDS) + 1)):
            raise ValueError("invalid inactive observation")
    elif not 0 <= character < 5:
        raise ValueError("invalid observation character")
    else:
        sts2_sim.validate_packed_observation(character, globals_, counts, exact, actions, digest)


def tensors(observation, target, model):
    pack_batch(observation)
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
    if globals_.shape != (batch, 3605) or not np.isfinite(globals_).all() \
            or digests.dtype != np.uint64 or digests.shape != (batch,):
        raise ValueError("invalid observation digests")
    action_row, action_position = np.nonzero(represented)
    action_counts = np.bincount(action_row, minlength=batch)
    action_offsets = np.cumsum(action_counts) - action_counts
    remap = np.full(represented.shape, -1, np.int32)
    remap[action_row, action_position] = np.arange(len(action_row)) - np.repeat(
        action_offsets, action_counts
    )
    counts = np.empty((batch, len(DOMAIN_SPECS)), np.uint32)
    pieces = [[] for _ in range(batch)]
    for domain, ((name, unsigned, signed, semantic, numeric), values) in enumerate(
        zip(DOMAIN_SPECS, observation[2])
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


def pack_batch(observation):
    rows = _pack_batch(observation)
    if len(rows) != len(observation[0]):
        raise ValueError("packed observation row count mismatch")
    for row in rows:
        validate_packed(row)
    return rows


def pack(observation, index, model):
    return pack_batch(observation)[index]


def unpack(rows, target, model):
    for row in rows:
        validate_packed(row)
    batch = len(rows); lengths = np.asarray([len(row[4]) for row in rows], np.int32)
    actions = max(1, int(lengths.max()))
    domains = []
    offsets = np.cumsum(lengths) - lengths
    counts_by_domain = np.stack([row[2] for row in rows]).astype(np.int32)
    exact = np.concatenate([row[3] for row in rows])
    widths = np.asarray([unsigned + signed + semantic + numeric + 1
                         for _name, unsigned, signed, semantic, numeric in DOMAIN_SPECS])
    words = counts_by_domain * widths
    row_starts = np.cumsum(words.sum(1)) - words.sum(1)
    starts = row_starts[:, None] + np.cumsum(words, 1) - words
    for domain, (_name, unsigned, signed, semantic, numeric) in enumerate(DOMAIN_SPECS):
        width = unsigned + signed + semantic + numeric + 1
        counts = counts_by_domain[:, domain]
        sizes = words[:, domain]
        positions = np.arange(sizes.sum()) - np.repeat(np.cumsum(sizes) - sizes, sizes)
        values = exact[np.repeat(starts[:, domain], sizes) + positions].reshape(-1, width)
        row_index = np.repeat(np.arange(batch, dtype=np.int32), counts)
        scope = values[:, -1].view(np.int32).copy()
        candidate = scope >= 0
        scope[candidate] += np.repeat(offsets, counts)[candidate]
        domains.append((
            values[:, :unsigned], values[:, unsigned:unsigned + signed].view(np.int32),
            values[:, unsigned + signed:unsigned + signed + semantic],
            values[:, unsigned + signed + semantic:-1].view(np.float32), row_index, scope,
        ))
    action_row = np.repeat(np.arange(batch, dtype=np.int32), lengths)
    action_position = np.arange(lengths.sum(), dtype=np.int32) - np.repeat(offsets, lengths)
    action = np.concatenate([row[4] for row in rows])
    legal = np.zeros((batch, actions), bool)
    legal[action_row, action_position] = action[:, -1].astype(bool)
    return model_inputs(
        np.asarray([row[0] for row in rows], np.uint8),
        np.asarray([row[1] for row in rows], np.float32), domains,
        (action[:, :ACTION_FIELDS[0]],
         action[:, ACTION_FIELDS[0]:sum(ACTION_FIELDS[:2])].view(np.int32),
         action[:, sum(ACTION_FIELDS[:2]):sum(ACTION_FIELDS[:3])],
         action[:, sum(ACTION_FIELDS[:3]):sum(ACTION_FIELDS)].view(np.float32)),
        action_row, action_position, legal, target, model,
    )


def minibatch_scores(rows, model):
    return [
        (int(row[2].sum()) + len(row[4])) * model.width
        + model.heads * (len(row[4]) + 1)
        for row in rows
    ]


class WinningReservoir:
    def __init__(self, capacity, envs, pending_capacity=None):
        self.capacity = capacity
        self.pending_capacity = pending_capacity or max(1, capacity // envs)
        self.pending = [[] for _ in range(envs)]
        self.pending_seen = [0] * envs
        self.rows = []
        self.completed = []
        self.character_seen = [0] * 5
        self.seen = self.wins = self.skipped = self.forced = 0

    def record(self, rows, choices, log_probabilities, policies, values, rng):
        for index, (row, choice, log_probability, policy, value) in enumerate(
            zip(rows, choices, log_probabilities, policies, values)
        ):
            if np.count_nonzero(row[4][:, -1]) <= 1:
                self.forced += 1
                continue
            episode = self.pending[index]
            self.pending_seen[index] += 1
            item = (row, int(choice), float(log_probability), 1.0 - float(value),
                    np.asarray(policy[:len(row[4])], np.float16))
            if len(episode) < self.pending_capacity:
                episode.append(item)
            else:
                slot = int(rng.integers(self.pending_seen[index]))
                if slot < self.pending_capacity:
                    episode[slot] = item

    def finish(self, indices, won, rng):
        for index, victory in zip(indices, won):
            episode = self.pending[index]
            if victory:
                self.wins += 1
                for row, action, probability, advantage, policy in episode:
                    if advantage <= 0:
                        self.skipped += 1
                        continue
                    self.completed.append(
                        (row, action, probability, min(advantage, 1.0), policy)
                    )
            episode.clear()
            self.pending_seen[index] = 0

    def admit(self, rows, wins, skipped, forced, rng):
        self.wins += wins
        self.skipped += skipped
        self.forced += forced
        groups = self.by_character(None)
        added = 0
        for item in rows:
            character = int(item[0][0])
            quota = self.capacity // 5 + (character < self.capacity % 5)
            self.seen += 1
            self.character_seen[character] += 1
            if len(groups[character]) < quota:
                groups[character].append(len(self.rows))
                self.rows.append(item)
                added += 1
                continue
            slot = int(rng.integers(self.character_seen[character]))
            if slot < quota:
                self.rows[groups[character][slot]] = item
                added += 1
        return added

    def evict(self, rows):
        rejected = {id(row) for row in rows}
        before = len(self.rows)
        self.rows = [row for row in self.rows if id(row) not in rejected]
        return before - len(self.rows)

    def discard(self, indices):
        for index in indices:
            self.pending[index].clear()
            self.pending_seen[index] = 0

    def clear(self):
        self.pending = [[] for _ in self.pending]
        self.pending_seen = [0] * len(self.pending_seen)
        self.rows.clear()
        self.completed.clear()
        self.character_seen = [0] * 5
        self.seen = self.wins = self.skipped = self.forced = 0

    def state_dict(self):
        return {
            "rows": list(self.rows), "seen": self.seen, "wins": self.wins,
            "skipped": self.skipped, "forced": self.forced,
            "character_seen": self.character_seen,
        }

    def load_state_dict(self, state):
        rows = [row if len(row) == 5 else (*row, None) for row in state.get("rows", ())]
        groups = [[] for _ in range(5)]
        for row in rows:
            character = int(row[0][0])
            if len(groups[character]) < self.capacity // 5 + (character < self.capacity % 5):
                groups[character].append(row)
        self.rows = [row for group in groups for row in group]
        self.completed.clear()
        self.seen = max(len(self.rows), state.get("seen", 0))
        self.wins = state.get("wins", 0)
        self.skipped = state.get("skipped", 0)
        self.forced = state.get("forced", 0)
        seen = list(state.get("character_seen", map(len, groups)))
        self.character_seen = [max(len(group), seen[index]) for index, group in enumerate(groups)]

    def by_character(self, character_start):
        groups = [[] for _ in range(5)]
        for index, (row, *_rest) in enumerate(self.rows):
            character = int(row[0])
            groups[character].append(index)
        return groups

    def drain_candidates(self):
        result = {
            "rows": self.completed, "wins": self.wins,
            "skipped": self.skipped, "forced": self.forced,
        }
        self.completed = []
        self.wins = self.skipped = self.forced = 0
        return result

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


def act(model, observation, target, sample, precision, generator=None, temperature=1):
    with torch.inference_mode():
        inputs = tensors(observation, target, model)
        logits, value_logit, progress_value = predict(model, inputs, precision, temperature)
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
        return dict.fromkeys((
            "min", "p01", "p05", "p10", "p25", "median", "p75", "p90", "p95", "p99",
            "max", "mean",
        ))
    points = np.quantile(values, [0, .01, .05, .1, .25, .5, .75, .9, .95, .99, 1])
    return dict(zip(("min", "p01", "p05", "p10", "p25", "median", "p75", "p90", "p95", "p99", "max"), map(float, points))) | {"mean": float(np.mean(values))}


def wilson(wins, runs):
    if not runs:
        return [0.0, 1.0]
    z = 1.96
    rate = wins / runs
    center = (rate + z * z / (2 * runs)) / (1 + z * z / runs)
    radius = z * math.sqrt(
        rate * (1 - rate) / runs + z * z / (4 * runs * runs)
    ) / (1 + z * z / runs)
    return [max(0, center - radius), min(1, center + radius)]


def episode_summary(rows):
    wins = sum(row[0] for row in rows)
    floors = [row[1] for row in rows]
    boss_entries = sum(row[0] or row[1] >= 51 for row in rows)
    return {
        "episodes": len(rows), "wins": wins,
        "win_rate": wins / max(1, len(rows)), "win_rate_interval": wilson(wins, len(rows)),
        "floor_mean": float(np.mean(floors)) if floors else None, "floor_bands": bands(floors),
        "boss_entries": boss_entries,
        "boss_entry_rate": boss_entries / max(1, len(rows)),
        "boss_entry_rate_interval": wilson(boss_entries, len(rows)),
        "boss_conversion": wins / max(1, boss_entries),
        "boss_conversion_interval": wilson(wins, boss_entries),
        "caps": sum(row[2] or row[3] or row[4] for row in rows),
        "step_caps": sum(row[2] for row in rows),
        "combat_caps": sum(row[3] for row in rows),
        "empty_actions": sum(row[4] for row in rows),
    }


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
    return [episode_summary(rows) for rows in episodes]


def curriculum_weights(stage, stage_decisions, auxiliary_decisions, args, active=True):
    stage_fraction = min(1, stage_decisions / args.progress_decisions)
    progress_fraction = min(1, auxiliary_decisions / args.progress_decisions)
    scale = max(.35, .8 ** stage)
    beta = 0 if not active else (
        args.progress_beta if args.progress_beta is not None
        else 1 - .9 * progress_fraction
    )
    entropy_start, entropy_end = args.entropy_start * scale, args.entropy_end * scale
    return beta, entropy_start + (entropy_end - entropy_start) * stage_fraction, {
        "stage_decisions": stage_decisions, "auxiliary_decisions": auxiliary_decisions,
        "fraction": stage_fraction, "progress_fraction": progress_fraction,
        "progress_beta_start": 1 if active else 0,
        "progress_beta_end": .1 if active else 0,
        "entropy_start": entropy_start, "entropy_end": entropy_end,
    }


def resume_stage_decisions(source, stage):
    if not source:
        return 0
    auxiliary = source.get("auxiliary_decisions", source["decisions"])
    if not stage:
        return auxiliary
    if "stage_decisions" not in source:
        raise ValueError("checkpoint lacks the current-stage decision clock")
    stage_decisions = source["stage_decisions"]
    if not 0 <= stage_decisions <= auxiliary:
        raise ValueError("invalid current-stage decision clock")
    return stage_decisions


def cuts(done, episode_steps, combat_steps, still_combat, legal, max_steps, max_combat_steps):
    step = episode_steps >= max_steps
    combat = (combat_steps >= max_combat_steps) & still_combat
    empty = ~legal.any(1)
    cut = (step | combat | empty) & ~done
    return cut, step & cut, combat & cut, empty & cut


def sampling_steps(produced, budget, envs, limit):
    return max(0, min(limit, (budget - produced + envs - 1) // envs))


class RolloutCollector:
    def __init__(self, args, sampler_session, stage, reservoir, iteration=0, worker=0,
                 generation=0):
        ascension, bonus = STAGES[stage]
        self.args = args
        seed_worker = worker + generation * args.samplers
        self.env = sts2_sim.Batch(
            args.envs, args.training_seed + sampler_session * 10_000_000 + seed_worker * args.envs,
            None,
            ascension=ascension,
        )
        self.env.set_training_bonus(bonus)
        self.observation = self.env.observe_tokens(flat=True)
        self.episode_steps = np.zeros(args.envs, np.int32)
        self.combat_steps = np.zeros(args.envs, np.int32)
        self.max_floor = np.asarray(
            [(row[0] - 1) * 17 + row[1] for row in self.env.stats()], np.float32
        )
        self.reservoir = reservoir
        seed = args.seed + sampler_session * 10_000_000 + seed_worker
        self.rng = np.random.default_rng(seed)
        self.torch_rng = torch.Generator().manual_seed(seed)
        self.iteration, self.sampler_session, self.stage = iteration, sampler_session, stage
        self.worker, self.generation = worker, generation
        self.trajectories = [None] * args.envs
        self.action_history = [[] for _ in range(args.envs)]

    def trace_empty(self, kind, indices, characters, stats):
        seeds = self.env.seeds()
        with (Path(self.args.output) / "empty-actions.jsonl").open("a") as output:
            for index in indices:
                output.write(json.dumps({
                    "kind": kind, "sampler_session": self.sampler_session, "worker": self.worker,
                    "iteration": self.iteration, "env": index, "seed": seeds[index],
                    "character": int(characters[index]), "stage": self.stage,
                    "stats": list(stats[index]), "actions": self.action_history[index],
                }) + "\n")

    def collect(self, model, target, precision, deadline, steps, version, stop=None,
                heartbeat=None, progress=None):
        args = self.args
        cache_start = model.cache_stats.copy()
        finished = []
        episodes = [[] for _ in range(5)]
        discarded_steps = orphan_empty_actions = sampled_steps = 0
        collect_seconds = 0.0
        for _ in range(steps):
            if time.monotonic() >= deadline or stop and stop.is_set():
                break
            if heartbeat is not None:
                heartbeat[self.worker] = time.monotonic()
            self.iteration += 1
            empty = ~observation_legal(self.observation).any(1)
            if empty.any():
                reset = np.flatnonzero(empty).tolist()
                stale_characters = np.asarray(self.observation[0], np.uint8)
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
                self.observation = self.env.observe_tokens(flat=True)
            step_started = time.monotonic()
            characters = np.asarray(self.observation[0], np.uint8)
            choice, log_probability, policy, value, progress_value = act(
                model, self.observation, target, True, precision, self.torch_rng,
                args.policy_temperature,
            )
            progress_floor = self.max_floor / 52
            for index, action in enumerate(choice):
                self.action_history[index].append(int(action))
            step_rows = pack_batch(self.observation)
            self.reservoir.record(step_rows, choice, log_probability, policy, value, self.rng)
            in_combat = np.asarray([row[4] == 1 for row in self.env.stats()])
            self.combat_steps = np.where(in_combat, self.combat_steps + 1, 0)
            raw_reward, done, _ = self.env.step(choice.tolist())
            raw_reward = np.array(raw_reward, np.float32, copy=True)
            done = np.asarray(done, bool)
            sampled_steps += args.envs
            if progress is not None:
                progress[self.worker] += args.envs
            self.episode_steps += 1
            next_observation = self.env.observe_tokens((~done).tolist(), True)
            stats = self.env.stats()
            floors = np.asarray([(row[0] - 1) * 17 + row[1] for row in stats], np.float32)
            next_max_floor = np.maximum(np.maximum(self.max_floor, floors), 52 * raw_reward)
            still_combat = np.asarray([row[4] == 1 for row in stats])
            truncated, step_truncated, combat_truncated, empty_actions = cuts(
                done, self.episode_steps, self.combat_steps, still_combat,
                observation_legal(next_observation), args.max_steps, args.max_combat_steps,
            )
            boundary = done | truncated
            if empty_actions.any():
                reset = np.flatnonzero(empty_actions).tolist()
                self.trace_empty("post_step", reset, characters, stats)
            progress_reward = (next_max_floor - self.max_floor) / 52
            self.max_floor = next_max_floor
            for index, row in enumerate(step_rows):
                trajectory = self.trajectories[index]
                if trajectory is None:
                    trajectory = {key: [] for key in (
                        "rows", "choices", "old_log", "values", "progress_values",
                        "progress_floors", "win_rewards",
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
                    ("progress_rewards", progress_reward[index]),
                    ("terminals", done[index]), ("characters", characters[index]),
                    ("versions", version),
                ):
                    trajectory[key].append(item)
            if boundary.any():
                reset = np.flatnonzero(boundary).tolist()
                wins = [bool(done[index] and stats[index][4] == 12) for index in reset]
                self.reservoir.finish(reset, wins, self.rng)
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
                next_observation = self.env.observe_tokens(flat=True)
            segment = [
                index for index, trajectory in enumerate(self.trajectories)
                if trajectory is not None and args.segment_steps
                and len(trajectory["rows"]) >= args.segment_steps
            ]
            if segment:
                with torch.inference_mode():
                    next_inputs = tensors(next_observation, target, model)
                    _, bootstrap_value, bootstrap_progress = predict(model, next_inputs, precision)
                    bootstrap_value = bootstrap_value.sigmoid().cpu().numpy()
                    bootstrap_progress = bootstrap_progress.cpu().numpy()
                for index in segment:
                    trajectory = self.trajectories[index]
                    trajectory["bootstrap_value"] = bootstrap_value[index]
                    trajectory["bootstrap_progress"] = (
                        bootstrap_progress[index] - self.max_floor[index] / 52
                    )
                    trajectory["bootstrap_version"] = version
                    trajectory["completion_seconds"] = time.monotonic() - trajectory.pop("started")
                    finished.append(trajectory); self.trajectories[index] = None
            self.observation = next_observation
            collect_seconds += time.monotonic() - step_started
            if finished:
                break
        return {
            "trajectories": finished,
            "episodes": episodes,
            "iteration": self.iteration,
            "orphan_empty_actions": orphan_empty_actions,
            "collect_seconds": collect_seconds, "discarded_steps": discarded_steps,
            "sampled_steps": sampled_steps,
            **{key: model.cache_stats[key] - cache_start[key] for key in model.cache_stats},
        }

    def run(self, model, models, samples, stop, deadline, budget, worker=0,
            heartbeat=None, progress=None, version=0):
        produced = 0
        def empty():
            return {
                "trajectories": [], "episodes": [[] for _ in range(5)],
                "iteration": self.iteration, "orphan_empty_actions": 0,
                "collect_seconds": 0.0, "queue_full_waits": 0,
                "queue_put_seconds": 0.0,
                "discarded_steps": 0, "sampled_steps": 0,
                **dict.fromkeys(("card_hit", "card_miss", "graph_hit", "graph_miss"), 0),
            }
        pending = empty()
        while produced < budget and time.monotonic() < deadline and not stop.is_set():
            latest = None
            while True:
                try:
                    latest = models.get_nowait()
                except (Empty, EOFError, OSError):
                    break
            if latest:
                version, state = latest
                model.load_state_dict(state)
            steps = sampling_steps(produced, budget, self.args.envs, self.args.sampler_steps)
            if not steps:
                break
            result = self.collect(
                model, torch.device("cpu"), "fp32", deadline, steps, version, stop,
                heartbeat, progress,
            )
            pending["trajectories"].extend(result["trajectories"])
            for target, rows in zip(pending["episodes"], result["episodes"]):
                target.extend(rows)
            for key in (
                "orphan_empty_actions", "collect_seconds",
                "discarded_steps", "sampled_steps", "card_hit", "card_miss",
                "graph_hit", "graph_miss",
            ):
                pending[key] += result[key]
            pending["iteration"] = result["iteration"]
            if not pending["trajectories"]:
                continue
            produced += sum(len(trajectory["rows"]) for trajectory in pending["trajectories"])
            pending["reservoir"] = self.reservoir.drain_candidates()
            pending["queued_at"] = time.monotonic()
            put_started = pending["queued_at"]
            while not stop.is_set():
                try:
                    pending["queue_put_seconds"] = time.monotonic() - put_started
                    samples.put_nowait((worker, version, pending))
                    pending = empty()
                    break
                except Full:
                    pending["queue_full_waits"] += 1
                    if heartbeat is not None:
                        heartbeat[worker] = time.monotonic()
                    time.sleep(.01)
        pending["discarded_steps"] += sum(
            len(trajectory["rows"]) for trajectory in self.trajectories
            if trajectory is not None
        ) + sum(len(trajectory["rows"]) for trajectory in pending["trajectories"])
        pending["trajectories"].clear()
        active = [index for index, trajectory in enumerate(self.trajectories) if trajectory is not None]
        self.reservoir.discard(active)
        changes = self.reservoir.drain_candidates()
        if "reservoir" in pending:
            for key in changes:
                changes[key] = pending["reservoir"][key] + changes[key]
        pending["reservoir"] = changes
        pending["iteration"] = self.iteration
        if heartbeat is not None:
            heartbeat[worker] = time.monotonic()
        return worker, version, pending


def collect_worker(model, args, sampler_session, stage, capacity, pending_capacity, iteration,
                   worker, generation, version, models, samples, stop, deadline, budget, results,
                   heartbeat, progress):
    os.environ["RAYON_NUM_THREADS"] = str(args.sampler_threads)
    torch.set_num_threads(args.torch_threads)
    torch.set_num_interop_threads(1)
    reservoir = WinningReservoir(capacity, args.envs, pending_capacity)
    collector = RolloutCollector(
        args, sampler_session, stage, reservoir, iteration, worker, generation,
    )
    heartbeat[worker] = time.monotonic()
    results.put(collector.run(
        model, models, samples, stop, deadline, budget, worker, heartbeat, progress, version,
    ))


class ExperienceDataset:
    def __init__(self):
        self.rows = []
        self.data = {
            "action": np.empty(0, np.int64), "old": np.empty(0, np.float32),
            "advantage": np.empty(0, np.float32), "progress_advantage": np.empty(0, np.float32),
            "returns": np.empty(0, np.float32), "progress_returns": np.empty(0, np.float32),
            "character": np.empty(0, np.int8), "priority": np.empty(0, np.float32),
            "version": np.empty(0, np.int64),
        }
        self.seen = self.stale_dropped = self.ratio_dropped = self.kl_dropped = self.post_kl_dropped = 0

    def __len__(self):
        return len(self.rows)

    def add(self, result, args, model, progress_active=True):
        trajectories = result["trajectories"]
        if not trajectories:
            return 0
        fields = (
            "rows", "choices", "old_log", "values", "progress_values",
            "progress_floors", "win_rewards", "progress_rewards",
            "terminals", "characters", "versions",
        )
        rows = [item for trajectory in trajectories for item in trajectory["rows"]]
        reward = np.asarray([
            item for trajectory in trajectories for item in trajectory["win_rewards"]
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
        advantage = np.zeros_like(reward)
        progress_value_advantage = np.zeros_like(reward); progress_advantage = np.zeros_like(reward)
        end = 0
        for trajectory in trajectories:
            length = len(trajectory["rows"])
            if not length or any(len(trajectory[key]) != length for key in fields):
                raise ValueError("invalid trajectory fields")
            start, end = end, end + length
            bootstrap = trajectory.get("bootstrap_value")
            if (terminal[start:end - 1].any() or (not terminal[end - 1] and bootstrap is None)
                    or (terminal[end - 1] and bootstrap is not None)
                    or (bootstrap is not None
                        and trajectory.get("bootstrap_version") != trajectory["versions"][-1])):
                raise ValueError("invalid trajectory terminal")
            gae = progress_gae = 0.0
            next_value = float(bootstrap or 0)
            next_progress = float(trajectory.get("bootstrap_progress") or 0)
            for step in reversed(range(start, end)):
                live = not terminal[step]
                gae = reward[step] + live * next_value - value[step] + args.gae_lambda * live * gae
                progress_gae = (
                    progress_reward[step] + args.progress_gamma * live * next_progress
                    - remaining_progress[step]
                    + args.progress_gamma * args.gae_lambda * live * progress_gae
                )
                advantage[step] = gae
                progress_value_advantage[step] = progress_gae
                progress_advantage[step] = progress_gae
                next_value = value[step]; next_progress = remaining_progress[step]
        returns = np.clip(advantage + value, 0, 1)
        progress_returns = progress_value_advantage + remaining_progress + progress_floor
        priority = 1 + np.abs(advantage) + progress_active * np.abs(progress_advantage) \
            + 4 * terminal + 4 * (returns > .5)
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
        pool = np.arange(len(self))
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
                 base_decisions, auxiliary_decisions, stage_decisions, run_started, reservoir, progress_active,
                 save_report, save_step):
    ascension, bonus = STAGES[stage]
    collector_args = copy.copy(args)
    pending_capacity = max(1, reservoir.capacity // args.envs)
    collector_args.envs //= args.samplers
    dataset = ExperienceDataset()
    episodes = [[] for _ in range(5)]
    losses = {key: [] for key in ("mean_advantage", "policy_loss", "value_loss", "progress_value_loss", "progress_beta", "entropy", "entropy_weight", "kl", "post_kl", "clip_fraction", "winning_loss", "winning_kl")}
    pipeline = [
        f"{args.samplers} continuous CPU actor{'s' if args.samplers > 1 else ''} → "
        + (f"{args.segment_steps}-decision bootstrapped segments" if args.segment_steps
           else "complete terminal trajectories"),
        "Bounded queue → policy-lag and action-ratio freshness filters",
        "CPU-prefetched, character-balanced, advantage-prioritized full batches",
        f"{model.layers}-layer card encoder + sparse actor/map summaries → direct state heads",
        f"Asynchronous clipped PPO + weights published every {args.publish_updates} updates",
    ]
    last_batch = None
    winning_behavior_drift = 0
    decisions = discarded_steps = handled = sampled = attempted = forced = trained = updates = windows = 0
    segmented_trajectories = 0
    winning_added = winning_replayed = winning_rejected = orphan_empty_actions = post_kl_checks = 0
    policy_update_attempts = pre_kl_rejected_updates = post_kl_discarded_updates = 0
    post_kl_proposals = post_kl_rejected_proposals = post_kl_retries = post_kl_retry_depth = 0
    critic_only_updates = 0
    accepted_pre_kl_sum = accepted_post_kl_sum = 0.0
    winning_replayed_characters = [0] * 5
    winning_evicted_characters = [0] * 5
    trajectory_lengths = []
    trajectory_policy_spans = []
    trajectory_arrival_lags = []
    trajectory_stale_steps = []
    trajectory_seconds = []
    update_durations = []
    unpack_durations = []
    forward_durations = []
    backward_durations = []
    collect_seconds = update_seconds = 0.0
    collector_seconds = [0.0] * args.samplers
    worker_accounted = [0] * args.samplers
    sampler_generations = [0] * args.samplers
    sampler_restarts = [0] * args.samplers
    sampler_wedges = [0] * args.samplers
    sampler_exhausted = [False] * args.samplers
    watchdog_dropped = queue_full_waits = 0
    queue_put_seconds = queue_delay_sum = 0.0
    queue_packets = queue_peak = 0
    cache_stats = dict.fromkeys(("card_hit", "card_miss", "graph_hit", "graph_miss"), 0)
    observed_kl = observed_clip = 0.0
    latest_sampler_version = dataset_peak = 0
    sampler_versions = [0] * args.samplers
    sampler_iterations = [base_decisions // args.envs] * args.samplers
    latest_sampler_iteration = base_decisions // args.envs
    policy_lags = []
    reported_episodes = [0] * 5
    reported_metrics = {key: 0 for key in losses}
    reported_steps = reported_trajectories = 0
    reported_seconds = 0.0
    started = time.monotonic()
    rng = np.random.default_rng(args.seed + sampler_session + 1_000_000_000)
    context = multiprocessing.get_context("spawn")
    models = [context.Queue(maxsize=1) for _ in range(args.samplers)]
    samples = context.Queue(maxsize=2)
    results = context.Queue(); stop = context.Event()
    heartbeat = context.Array("d", [started] * args.samplers)
    progress = context.Array("q", [0] * args.samplers)
    packer = ThreadPoolExecutor(max_workers=2)
    workers = [None] * args.samplers

    def start_worker(worker):
        heartbeat[worker] = time.monotonic()
        progress[worker] = 0
        actor = Agent(
            model.layout, model.width, model.layers, model.heads, model.feedforward, model.head_width,
        ).cpu().eval()
        actor.load_state_dict({key: value.detach().cpu() for key, value in model.state_dict().items()})
        process = context.Process(target=collect_worker, args=(
            actor, collector_args, sampler_session, stage,
            reservoir.capacity, pending_capacity, sampler_iterations[worker], worker,
            sampler_generations[worker], updates, models[worker], samples, stop, deadline, budget,
            results, heartbeat, progress,
        ))
        workers[worker] = process
        process.start()

    for worker in range(args.samplers):
        start_worker(worker)

    def publish():
        if stop.is_set():
            return
        state = None
        for queue in models:
            try:
                if state is None:
                    state = {key: value.detach().cpu().clone() for key, value in model.state_dict().items()}
                queue.put_nowait((updates, state))
            except Full:
                pass

    def ingest(item):
        nonlocal decisions, discarded_steps, sampled, collect_seconds, winning_added, orphan_empty_actions, latest_sampler_version, latest_sampler_iteration, dataset_peak, segmented_trajectories, queue_full_waits, queue_put_seconds, queue_delay_sum, queue_packets, queue_peak
        worker, version, result = item
        sampler_versions[worker] = version
        sampler_iterations[worker] = result["iteration"]
        latest_sampler_version = min(sampler_versions)
        latest_sampler_iteration = max(sampler_iterations)
        decisions += dataset.add(result, args, model, progress_active)
        if decisions >= budget:
            stop.set()
        sampled += result["sampled_steps"]
        worker_accounted[worker] += result["sampled_steps"]
        discarded_steps += result["discarded_steps"]
        for trajectory in result["trajectories"]:
            segmented_trajectories += int(not trajectory["terminals"][-1])
            trajectory_lengths.append(len(trajectory["rows"]))
            trajectory_policy_spans.append(
                max(trajectory["versions"]) - min(trajectory["versions"])
            )
            trajectory_arrival_lags.append(max(0, updates - min(trajectory["versions"])))
            trajectory_stale_steps.append(sum(
                version < updates - args.max_policy_lag for version in trajectory["versions"]
            ))
            trajectory_seconds.append(trajectory["completion_seconds"])
        dataset_peak = max(dataset_peak, len(dataset))
        update = result["reservoir"]
        winning_added += reservoir.admit(
            update["rows"], update["wins"], update["skipped"], update["forced"], rng,
        )
        for target_episodes, collected in zip(episodes, result["episodes"]):
            target_episodes.extend(collected)
        collector_seconds[worker] += result["collect_seconds"]
        collect_seconds = max(collector_seconds)
        for key in cache_stats:
            cache_stats[key] += result[key]
        orphan_empty_actions += result["orphan_empty_actions"]
        queue_full_waits += result.get("queue_full_waits", 0)
        queue_put_seconds += result.get("queue_put_seconds", 0)
        if "queued_at" in result:
            queue_delay_sum += time.monotonic() - result["queued_at"]
            queue_packets += 1
        try:
            queue_peak = max(queue_peak, samples.qsize())
        except NotImplementedError:
            pass

    def upload(value):
        if torch.is_tensor(value):
            return value.to(target, non_blocking=True)
        if isinstance(value, tuple):
            return tuple(upload(item) for item in value)
        return value

    def reserve_batch(size):
        index = dataset.sample(size, rng)
        rows = [dataset.rows[i] for i in index]
        values = {key: dataset.data[key][index].copy() for key in dataset.data}
        dataset.discard(index)
        replay_limit = len(index) // 9
        replay = reservoir.sample(replay_limit, rng, model.character_start) if replay_limit else []
        packed = packer.submit(
            unpack, rows + [sample[0] for sample in replay], torch.device("cpu"), model
        )
        return rows, values, replay, packed

    def optimizer_metrics():
        return {
            "policy_rows_attempted": attempted,
            "policy_rows_forced": forced,
            "policy_rows_accepted": trained,
            "policy_rows_discarded": attempted - trained,
            "ratio_rejected_rows": dataset.ratio_dropped,
            "pre_kl_rejected_rows": dataset.kl_dropped,
            "post_kl_discarded_rows": dataset.post_kl_dropped,
            "policy_update_attempts": policy_update_attempts,
            "policy_updates_accepted": updates,
            "pre_kl_rejected_updates": pre_kl_rejected_updates,
            "post_kl_proposals": post_kl_proposals,
            "post_kl_rejected_proposals": post_kl_rejected_proposals,
            "post_kl_retries": post_kl_retries,
            "post_kl_retry_depth_max": post_kl_retry_depth,
            "post_kl_discarded_updates": post_kl_discarded_updates,
            "critic_only_updates": critic_only_updates,
            "accepted_pre_kl_sum": accepted_pre_kl_sum,
            "accepted_post_kl_sum": accepted_post_kl_sum,
            "accepted_kl_count": updates,
            "dataset_ultimately_discarded": attempted - trained,
        }

    def report():
        nonlocal observed_kl, observed_clip, reported_steps, reported_seconds, reported_trajectories
        if last_batch:
            rows, action, old = last_batch
            inputs = unpack(rows, target, model)
            action = torch.as_tensor(action, device=target)
            old = torch.as_tensor(old, device=target)
            model.eval()
            with torch.no_grad():
                logits = predict(model, inputs, args.precision, args.policy_temperature)[0]
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
        recent_summary = episode_summary(recent)
        reported_episodes[:] = map(len, episodes)
        elapsed = time.monotonic() - run_started
        segment_elapsed = time.monotonic() - started
        recent_metrics = metric_means({
            key: values[reported_metrics[key]:]
            for key, values in losses.items() if len(values) > reported_metrics[key]
        })
        recent_lengths = trajectory_lengths[reported_trajectories:]
        recent_spans = trajectory_policy_spans[reported_trajectories:]
        recent_arrival_lags = trajectory_arrival_lags[reported_trajectories:]
        recent_stale_steps = trajectory_stale_steps[reported_trajectories:]
        recent_seconds = trajectory_seconds[reported_trajectories:]
        reported_trajectories = len(trajectory_lengths)
        for key, values in losses.items():
            reported_metrics[key] = len(values)
        replay_fraction = winning_replayed / max(1, trained + winning_replayed)
        assert replay_fraction <= .1 + 1e-9
        _, _, curriculum = curriculum_weights(
            stage, stage_decisions + handled, auxiliary_decisions + handled, args, progress_active
        )
        point = {
            "steps": base_decisions + handled, "sampled_decisions": base_decisions + sampled,
            "accepted_decisions": base_decisions + decisions,
            "iteration": latest_sampler_iteration, "samplers": args.samplers,
            "updates": updates, "episodes": len(recent),
            "wins": sum(row[0] for row in recent), "caps": sum(row[2] or row[3] or row[4] for row in recent),
            "win_rate": recent_summary["win_rate"],
            "win_rate_interval": recent_summary["win_rate_interval"],
            "boss_entries": recent_summary["boss_entries"],
            "boss_entry_rate": recent_summary["boss_entry_rate"],
            "boss_entry_rate_interval": recent_summary["boss_entry_rate_interval"],
            "boss_conversion": recent_summary["boss_conversion"],
            "boss_conversion_interval": recent_summary["boss_conversion_interval"],
            "seconds": elapsed,
            "decisions_per_second": (handled - reported_steps) / max(1e-9, segment_elapsed - reported_seconds),
            "trajectory_floors": [(row[5], row[1], character)
                                  for character, rows in enumerate(recent_by_character) for row in rows],
            "step_caps": sum(row[2] for row in recent), "combat_caps": sum(row[3] for row in recent),
            "empty_actions": sum(row[4] for row in recent),
            "collect_seconds": collect_seconds, "update_seconds": update_seconds,
            "actor_decisions_per_second": sampled / max(1e-9, collect_seconds),
            "learner_decisions_per_second": trained / max(1e-9, update_seconds),
            "row_utilization": trained / max(1, attempted),
            "update_seconds_p95": float(np.quantile(update_durations, .95)) if update_durations else 0,
            "unpack_seconds_p95": float(np.quantile(unpack_durations, .95)) if unpack_durations else 0,
            "forward_seconds_p95": float(np.quantile(forward_durations, .95)) if forward_durations else 0,
            "backward_seconds_p95": float(np.quantile(backward_durations, .95)) if backward_durations else 0,
            **cache_stats,
            "card_cache_hit_rate": cache_stats["card_hit"] / max(
                1, cache_stats["card_hit"] + cache_stats["card_miss"]
            ),
            "graph_cache_hit_rate": cache_stats["graph_hit"] / max(
                1, cache_stats["graph_hit"] + cache_stats["graph_miss"]
            ),
            "mps_allocated_bytes": torch.mps.current_allocated_memory() if target.type == "mps" else 0,
            "mps_driver_allocated_bytes": torch.mps.driver_allocated_memory() if target.type == "mps" else 0,
            "floor_bands": bands([row[1] for row in recent]),
            "dataset_rows": len(dataset), "dataset_peak": dataset_peak, "dataset_seen": dataset.seen,
            "dataset_attempted": attempted, "dataset_trained": trained,
            "dataset_stale_dropped": dataset.stale_dropped,
            "dataset_ratio_dropped": dataset.ratio_dropped, "dataset_kl_dropped": dataset.kl_dropped,
            "dataset_post_kl_dropped": dataset.post_kl_dropped,
            "post_kl_checks": post_kl_checks,
            **optimizer_metrics(),
            "discarded_steps": discarded_steps,
            "discarded_step_fraction": discarded_steps / max(1, sampled),
            "completed_trajectories": len(trajectory_lengths),
            "bootstrapped_segments": segmented_trajectories,
            "trajectory_length_mean": float(np.mean(recent_lengths)) if recent_lengths else 0,
            "trajectory_length_max": max(recent_lengths, default=0),
            "trajectory_policy_span_mean": float(np.mean(recent_spans)) if recent_spans else 0,
            "trajectory_policy_span_max": max(recent_spans, default=0),
            "trajectory_arrival_lag_mean": float(np.mean(recent_arrival_lags)) if recent_arrival_lags else 0,
            "trajectory_arrival_lag_max": max(recent_arrival_lags, default=0),
            "trajectory_stale_step_fraction": sum(recent_stale_steps) / max(1, sum(recent_lengths)),
            "trajectory_completion_seconds_mean": float(np.mean(recent_seconds)) if recent_seconds else 0,
            "trajectory_completion_seconds_max": max(recent_seconds, default=0),
            "policy_version": updates, "sampler_version": latest_sampler_version,
            "policy_lag_mean": float(np.mean(policy_lags)) if policy_lags else 0,
            "sampler_heartbeats": [time.monotonic() - value for value in heartbeat[:]],
            "sampler_restarts": sampler_restarts, "sampler_wedges": sampler_wedges,
            "watchdog_dropped_steps": watchdog_dropped,
            "sample_queue_capacity": 2, "sample_queue_peak": queue_peak,
            "sample_queue_packets": queue_packets,
            "sample_queue_delay_mean": queue_delay_sum / max(1, queue_packets),
            "sample_queue_full_waits": queue_full_waits,
            "sample_queue_put_seconds": queue_put_seconds,
            "winning_reservoir": len(reservoir.rows), "winning_seen": reservoir.seen,
            "winning_character_rows": list(map(len, reservoir.by_character(model.character_start))),
            "winning_episodes": reservoir.wins, "winning_added": winning_added,
            "winning_skipped": reservoir.skipped, "winning_forced_skipped": reservoir.forced,
            "winning_replayed": winning_replayed,
            "winning_rejected_kl": winning_rejected,
            "winning_replayed_characters": winning_replayed_characters,
            "winning_evicted_characters": winning_evicted_characters,
            "winning_replay_fraction": replay_fraction,
            "winning_loss_weight": args.winning_loss_weight,
            "winning_collection_drift": float(winning_behavior_drift),
            "observed_kl": observed_kl, "observed_clip_fraction": observed_clip,
            "progress_active": progress_active, "curriculum": curriculum,
            **recent_metrics,
        }
        reported_steps = handled
        reported_seconds = segment_elapsed
        point["characters"] = summaries([rows[-args.promotion_window:] for rows in episodes])
        save_report(point, pipeline, windows)
    next_report = base_decisions + args.report_decisions
    next_save = base_decisions + args.save_decisions
    promotion_ready = False
    pending = []
    def drain_results():
        while True:
            try:
                ingest(results.get_nowait())
            except (Empty, EOFError, OSError):
                return

    def restart_stalled():
        nonlocal sampled, discarded_steps, watchdog_dropped
        if stop.is_set() or time.monotonic() >= deadline or decisions >= budget:
            stop.set()
            return
        now = time.monotonic()
        for worker, process in enumerate(workers):
            if sampler_exhausted[worker]:
                continue
            wedged = process.is_alive() and now - heartbeat[worker] > args.sampler_timeout
            failed = not process.is_alive() and process.exitcode not in (None, 0)
            if not wedged and not failed:
                continue
            if wedged:
                sampler_wedges[worker] += 1
                process.terminate()
            process.join(5)
            if process.is_alive():
                process.kill(); process.join()
            while True:
                try:
                    ingest(samples.get_nowait())
                except (Empty, EOFError, OSError):
                    break
            drain_results()
            dropped = max(0, int(progress[worker]) - worker_accounted[worker])
            sampled += dropped
            discarded_steps += dropped
            watchdog_dropped += dropped
            worker_accounted[worker] = 0
            if (sampler_restarts[worker] >= args.sampler_restarts or stop.is_set()
                    or time.monotonic() >= deadline or decisions >= budget):
                sampler_exhausted[worker] = True
                continue
            sampler_restarts[worker] += 1
            sampler_generations[worker] += 1
            while True:
                try:
                    models[worker].get_nowait()
                except (Empty, EOFError, OSError):
                    break
            start_worker(worker)

    try:
        while True:
            drain_results()
            restart_stalled()
            sampler_done = (stop.is_set() or all(not worker.is_alive() for worker in workers)) and samples.empty()
            if not pending:
                if len(dataset) < args.batch and not sampler_done:
                    try:
                        ingest(samples.get(timeout=.1))
                    except (Empty, EOFError, OSError):
                        restart_stalled()
                        continue
                    sampler_done = (stop.is_set() or all(not worker.is_alive() for worker in workers)) and samples.empty()
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
                        and sum(row[0] for row in rows) / len(rows) >= args.promotion_trigger_rate
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
            while len(pending) < 2:
                if not sampler_done:
                    try:
                        ingest(samples.get_nowait())
                    except (Empty, EOFError, OSError):
                        pass
                    sampler_done = (stop.is_set() or all(not worker.is_alive() for worker in workers)) and samples.empty()
                queued = sum(len(batch[0]) for batch in pending)
                boundary = min(next_report, next_save) - (base_decisions + handled) - queued
                enough = len(dataset) >= args.batch or sampler_done and len(dataset)
                if boundary <= 0 or not enough:
                    break
                pending.append(reserve_batch(min(args.batch, boundary)))
            if not pending:
                if sampler_done and not len(dataset):
                    break
                continue
            update_started = time.monotonic()
            rows, values, replay, packed = pending.pop(0)
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
            policy_lags.extend((updates - values["version"]).tolist())
            if not rows:
                continue
            unpack_started = time.monotonic()
            inputs = upload(packed.result())
            unpack_seconds = time.monotonic() - unpack_started
            unpack_durations.append(unpack_seconds)
            action = torch.as_tensor(values["action"], device=target)
            old = torch.as_tensor(values["old"], device=target)
            forward_started = time.monotonic()
            all_logits, all_prediction, all_progress_prediction = predict(
                model, inputs, args.precision, args.policy_temperature,
            )
            forward_seconds = time.monotonic() - forward_started
            forward_durations.append(forward_seconds)
            backward_seconds = 0.0
            logits = all_logits[:len(rows)]
            prediction = all_prediction[:len(rows)]
            progress_prediction = all_progress_prediction[:len(rows)]
            legal = inputs[6][:len(rows)]
            distribution = torch.distributions.Categorical(
                logits=logits.masked_fill(~legal, -torch.inf)
            )
            log_ratio = distribution.log_prob(action) - old
            critic_fresh = log_ratio.abs() <= args.max_log_ratio
            actionable = legal.sum(1) > 1
            fresh = critic_fresh & actionable
            critic_mask = critic_fresh.to(log_ratio.dtype)
            mask = fresh.to(log_ratio.dtype)
            critic_count = critic_mask.sum()
            fresh_count = mask.sum()
            critic_denominator = critic_count.clamp_min(1)
            denominator = fresh_count.clamp_min(1)
            safe_log_ratio = torch.where(fresh, log_ratio, torch.zeros_like(log_ratio))
            ratio = safe_log_ratio.exp()
            kl = ((ratio - 1 - safe_log_ratio) * mask).sum().div(denominator).detach()
            fresh_cpu = fresh.detach().cpu().numpy()
            fresh_rows = int(fresh_cpu.sum())
            actionable_rows = int(actionable.sum())
            attempted += actionable_rows
            forced += len(rows) - actionable_rows
            dataset.ratio_dropped += actionable_rows - fresh_rows
            kl_value = float(kl)
            progress_beta, entropy_weight, _ = curriculum_weights(
                stage, stage_decisions + handled, auxiliary_decisions + handled, args, progress_active
            )
            batch_returns = torch.as_tensor(values["returns"], device=target)
            batch_progress_returns = torch.as_tensor(values["progress_returns"], device=target)
            value_loss = (nn.functional.binary_cross_entropy_with_logits(
                prediction, batch_returns, reduction="none"
            ) * critic_mask).sum() / critic_denominator
            progress_value_loss = (
                (progress_prediction - batch_progress_returns).square() * critic_mask
            ).sum() / critic_denominator
            critic_loss = args.value_weight * (
                value_loss + (progress_beta > 0) * progress_value_loss
            )
            critic_parameters = tuple(model.value.parameters()) + tuple(model.progress_value.parameters())
            if not critic_count:
                update_elapsed = time.monotonic() - update_started
                update_seconds += update_elapsed
                update_durations.append(update_elapsed)
                backward_durations.append(0.0)
                continue
            if not fresh_rows:
                backward_started = time.monotonic()
                optimizer.zero_grad(set_to_none=True); critic_loss.backward(inputs=critic_parameters)
                critic_only_step(model, optimizer, critic_parameters); publish()
                critic_only_updates += 1
                backward_seconds = time.monotonic() - backward_started
                backward_durations.append(backward_seconds)
                update_elapsed = time.monotonic() - update_started
                update_seconds += update_elapsed
                update_durations.append(update_elapsed)
                continue
            policy_update_attempts += 1
            if kl_value > args.target_kl:
                dataset.kl_dropped += fresh_rows
                pre_kl_rejected_updates += 1
                backward_started = time.monotonic()
                optimizer.zero_grad(set_to_none=True); critic_loss.backward(inputs=critic_parameters)
                critic_only_step(model, optimizer, critic_parameters); publish()
                critic_only_updates += 1
                backward_seconds = time.monotonic() - backward_started
                backward_durations.append(backward_seconds)
                update_elapsed = time.monotonic() - update_started
                update_seconds += update_elapsed
                update_durations.append(update_elapsed)
                continue
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
            policy_loss = -(torch.minimum(
                ratio * batch_advantage,
                ratio.clamp(1 - args.clip, 1 + args.clip) * batch_advantage,
            ) * mask).sum() / denominator
            entropy = (distribution.entropy() * mask).sum() / denominator
            loss = policy_loss + critic_loss - entropy_weight * entropy
            replay_valid = replay_eligible = eligible_cpu = valid_cpu = None
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
                replay_capacity = fresh_count.to(torch.int64) // 9
                replay_eligible = torch.arange(len(replay), device=target) < replay_capacity
                replay_valid = replay_eligible & (replay_kl <= args.target_kl)
                eligible_cpu = replay_eligible.cpu().numpy()
                valid_cpu = replay_valid.cpu().numpy()
                rejected = [sample for sample, eligible, keep in zip(
                    replay, eligible_cpu, valid_cpu,
                ) if eligible and not keep]
                reservoir.evict(rejected)
                winning_rejected += len(rejected)
                for sample in rejected:
                    winning_evicted_characters[int(sample[0][0])] += 1
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
                winning_loss = -args.winning_loss_weight * (torch.minimum(
                    replay_ratio * replay_advantage,
                    replay_ratio.clamp(1 - args.clip, 1 + args.clip) * replay_advantage,
                ) * replay_weight).sum() / replay_capacity.clamp_min(1)
                loss = loss + winning_loss
                replay_kl_mean = (replay_kl * replay_weight).sum() / replay_denominator
            backward_started = time.monotonic()
            optimizer.zero_grad(set_to_none=True); loss.backward()
            accepted, proposals = trust_region_step(
                model, optimizer, inputs, action, old, fresh, denominator,
                args.precision, args.target_kl, args.policy_temperature,
            )
            rejected_proposals = sum(
                not math.isfinite(value) or value > args.target_kl for value in proposals
            )
            post_kl_checks += len(proposals)
            post_kl_proposals += len(proposals)
            post_kl_rejected_proposals += rejected_proposals
            post_kl_retries += len(proposals) - 1
            post_kl_retry_depth = max(post_kl_retry_depth, len(proposals) - 1)
            if not accepted:
                dataset.post_kl_dropped += fresh_rows
                post_kl_discarded_updates += 1
                optimizer.zero_grad(set_to_none=True)
                retry_prediction, retry_progress = predict(
                    model, inputs, args.precision, args.policy_temperature,
                )[1:]
                retry_value_loss = (nn.functional.binary_cross_entropy_with_logits(
                    retry_prediction[:len(rows)], batch_returns, reduction="none"
                ) * critic_mask).sum() / critic_denominator
                retry_progress_loss = (
                    (retry_progress[:len(rows)] - batch_progress_returns).square() * critic_mask
                ).sum() / critic_denominator
                retry_critic_loss = args.value_weight * (
                    retry_value_loss + (progress_beta > 0) * retry_progress_loss
                )
                retry_critic_loss.backward(inputs=critic_parameters)
                critic_only_step(model, optimizer, critic_parameters); publish()
                critic_only_updates += 1
                update_elapsed = time.monotonic() - update_started
                update_seconds += update_elapsed
                update_durations.append(update_elapsed)
                backward_durations.append(time.monotonic() - backward_started)
                continue
            post_kl = proposals[-1]
            losses["post_kl"].append(post_kl)
            accepted_pre_kl_sum += kl_value
            accepted_post_kl_sum += post_kl
            clip_fraction = (((ratio - 1).abs() > args.clip).to(mask.dtype) * mask).sum().div(denominator).detach()
            losses["mean_advantage"].append(float(np.mean(
                values["advantage"][fresh_cpu] + progress_beta * values["progress_advantage"][fresh_cpu]
            )))
            losses["policy_loss"].append(policy_loss.detach())
            losses["value_loss"].append(value_loss.detach())
            losses["progress_value_loss"].append(progress_value_loss.detach())
            losses["progress_beta"].append(progress_beta)
            losses["entropy"].append(entropy.detach())
            losses["entropy_weight"].append(entropy_weight)
            losses["kl"].append(kl); losses["clip_fraction"].append(clip_fraction)
            trained += fresh_rows; updates += 1
            last_batch = (
                [row for row, selected in zip(rows, fresh_cpu) if selected],
                values["action"][fresh_cpu], values["old"][fresh_cpu],
            )
            if replay_valid is not None:
                replay_characters = [int(sample[0][0]) for sample in replay]
                for character, eligible, keep in zip(
                    replay_characters, eligible_cpu, valid_cpu
                ):
                    if eligible and keep:
                        winning_replayed_characters[character] += 1
                replay_count = int(valid_cpu.sum())
                if replay_count:
                    behavior = torch.as_tensor([sample[2] for sample in replay], device=target)
                    winning_behavior_drift = (
                        replay_log_probability.detach()[replay_valid] - behavior[replay_valid]
                    ).abs().mean()
                    losses["winning_loss"].append(winning_loss.detach())
                    losses["winning_kl"].append(replay_kl_mean.detach())
                    winning_replayed += replay_count
            if updates % args.publish_updates == 0:
                publish()
            backward_seconds = time.monotonic() - backward_started
            backward_durations.append(backward_seconds)
            update_elapsed = time.monotonic() - update_started
            update_seconds += update_elapsed
            update_durations.append(update_elapsed)
            if update_elapsed > 5:
                packed = rows + [sample[0] for sample in replay]
                scores = minibatch_scores(packed, model)
                represented_actions = max(len(row[4]) for row in packed)
                print(json.dumps({"slow_update": {
                    "seconds": update_elapsed,
                    "fresh": len(rows), "replay": len(replay),
                    "score_min": int(min(scores)), "score_max": int(max(scores)),
                    "state_tokens_max": max(int(row[2].sum()) for row in packed),
                    "represented_actions_max": represented_actions,
                    "state_attention_pairs": sum((int(row[2].sum()) + 1) ** 2 for row in packed),
                    "legal_actions_max": int(max(np.count_nonzero(row[4][:, -1]) for row in packed)),
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
                    and sum(row[0] for row in rows) / len(rows) >= args.promotion_trigger_rate
                    for rows in recent
                )
                if promotion_ready:
                    stop.set()
    finally:
        stop.set()
        packer.shutdown(wait=True, cancel_futures=True)
        started_workers = [worker for worker in workers if worker is not None and worker.pid is not None]
        shutdown_deadline = time.monotonic() + 5
        while any(worker.is_alive() for worker in started_workers) and time.monotonic() < shutdown_deadline:
            drain_results()
            for worker in started_workers:
                worker.join(.05)
        terminated = {worker for worker in started_workers if worker.is_alive()}
        for worker in terminated:
            worker.terminate()
        for worker in terminated:
            worker.join()
        drain_results()
        for index, worker in enumerate(workers):
            dropped = max(0, int(progress[index]) - worker_accounted[index])
            sampled += dropped
            discarded_steps += dropped
            watchdog_dropped += dropped * (worker in terminated)
            worker_accounted[index] += dropped
    failed = [worker.exitcode for worker in started_workers
              if worker not in terminated and worker.exitcode]
    if failed:
        raise RuntimeError(f"sampler processes failed: {failed}")
    assert handled == decisions and not len(dataset)
    assert sampled == decisions + discarded_steps
    if handled > reported_steps:
        windows += 1
        report()
    terminal = [episode for character in episodes for episode in character]
    terminal_summary = episode_summary(terminal)
    terminal_summary["empty_actions"] += orphan_empty_actions
    _, _, curriculum = curriculum_weights(
        stage, stage_decisions + decisions, auxiliary_decisions + decisions, args, progress_active
    )
    return {
        "decisions": decisions, "updates": updates, "windows": windows,
        "decisions_per_second": decisions / (time.monotonic() - started),
        "seconds": time.monotonic() - started,
        "collect_seconds": collect_seconds, "update_seconds": update_seconds,
        "actor_decisions_per_second": sampled / max(1e-9, collect_seconds),
        "learner_decisions_per_second": trained / max(1e-9, update_seconds),
        "row_utilization": trained / max(1, attempted),
        "update_seconds_p95": float(np.quantile(update_durations, .95)) if update_durations else 0,
        "unpack_seconds_p95": float(np.quantile(unpack_durations, .95)) if unpack_durations else 0,
        "forward_seconds_p95": float(np.quantile(forward_durations, .95)) if forward_durations else 0,
        "backward_seconds_p95": float(np.quantile(backward_durations, .95)) if backward_durations else 0,
        **cache_stats,
        "card_cache_hit_rate": cache_stats["card_hit"] / max(
            1, cache_stats["card_hit"] + cache_stats["card_miss"]
        ),
        "graph_cache_hit_rate": cache_stats["graph_hit"] / max(
            1, cache_stats["graph_hit"] + cache_stats["graph_miss"]
        ),
        "stage": {"index": stage, "ascension": ascension, "bonus": bonus},
        "characters": summaries(episodes),
        "terminals": terminal_summary,
        "description": f"Continuously trained V{MODEL_VERSION} on A{ascension}/+{bonus} trajectories.",
        "pipeline": pipeline, "promotion_ready": promotion_ready,
        "dataset_rows": len(dataset), "dataset_peak": dataset_peak, "dataset_seen": dataset.seen,
        "dataset_attempted": attempted, "dataset_trained": trained,
        "dataset_stale_dropped": dataset.stale_dropped,
        "dataset_ratio_dropped": dataset.ratio_dropped, "dataset_kl_dropped": dataset.kl_dropped,
        "dataset_post_kl_dropped": dataset.post_kl_dropped,
        "post_kl_checks": post_kl_checks,
        **optimizer_metrics(),
        "sampled_decisions": sampled, "discarded_steps": discarded_steps,
        "discarded_step_fraction": discarded_steps / max(1, sampled),
        "completed_trajectories": len(trajectory_lengths),
        "bootstrapped_segments": segmented_trajectories,
        "trajectory_length_mean": float(np.mean(trajectory_lengths)) if trajectory_lengths else 0,
        "trajectory_length_max": max(trajectory_lengths, default=0),
        "trajectory_policy_span_mean": float(np.mean(trajectory_policy_spans)) if trajectory_policy_spans else 0,
        "trajectory_policy_span_max": max(trajectory_policy_spans, default=0),
        "trajectory_arrival_lag_mean": float(np.mean(trajectory_arrival_lags)) if trajectory_arrival_lags else 0,
        "trajectory_arrival_lag_max": max(trajectory_arrival_lags, default=0),
        "trajectory_stale_step_fraction": sum(trajectory_stale_steps) / max(1, sum(trajectory_lengths)),
        "trajectory_completion_seconds_mean": float(np.mean(trajectory_seconds)) if trajectory_seconds else 0,
        "trajectory_completion_seconds_max": max(trajectory_seconds, default=0),
        "policy_version": updates, "sampler_version": latest_sampler_version,
        "policy_lag_mean": float(np.mean(policy_lags)) if policy_lags else 0,
        "sampler_heartbeats": [time.monotonic() - value for value in heartbeat[:]],
        "sampler_restarts": sampler_restarts, "sampler_wedges": sampler_wedges,
        "watchdog_dropped_steps": watchdog_dropped,
        "sample_queue_capacity": 2, "sample_queue_peak": queue_peak,
        "sample_queue_packets": queue_packets,
        "sample_queue_delay_mean": queue_delay_sum / max(1, queue_packets),
        "sample_queue_full_waits": queue_full_waits,
        "sample_queue_put_seconds": queue_put_seconds,
        "winning_reservoir": len(reservoir.rows), "winning_seen": reservoir.seen,
            "winning_character_rows": list(map(len, reservoir.by_character(model.character_start))),
        "winning_episodes": reservoir.wins, "winning_added": winning_added,
        "winning_skipped": reservoir.skipped, "winning_forced_skipped": reservoir.forced,
        "winning_replayed": winning_replayed,
        "winning_rejected_kl": winning_rejected,
        "winning_replayed_characters": winning_replayed_characters,
        "winning_evicted_characters": winning_evicted_characters,
        "winning_replay_fraction": winning_replayed / max(1, trained + winning_replayed),
        "winning_loss_weight": args.winning_loss_weight,
        "winning_collection_drift": float(winning_behavior_drift),
        "observed_kl": observed_kl, "observed_clip_fraction": observed_clip,
        "progress_active": progress_active, "curriculum": curriculum,
        **metric_means(losses),
    }


def evaluate(model, args, target, seed=None, runs=None, stage=None, max_steps=None, max_combat_steps=None):
    was_training = model.training
    model.eval()
    ascension, bonus = STAGES[stage] if stage is not None else (args.ascension, args.bonus)
    reports = []
    for character in range(5):
        outcomes = []; floors = []
        caps = step_caps = combat_caps = empty_count = 0
        total = runs or args.runs
        for start in range(0, total, args.evaluation_batch):
            count = min(args.evaluation_batch, total - start)
            base_seed = args.validation_seed if seed is None else seed
            env = sts2_sim.Batch(
                count, base_seed + character * 1_000_000 + start,
                character, ascension=ascension,
            )
            env.set_training_bonus(bonus)
            active = np.ones(count, bool)
            capped = np.zeros(count, bool)
            empty_actions = np.zeros(count, bool)
            step_capped = np.zeros(count, bool)
            combat_capped = np.zeros(count, bool)
            steps = np.zeros(count, np.int32)
            combat_steps = np.zeros(count, np.int32)
            for _ in range(max_steps or args.max_steps):
                if not active.any():
                    break
                observation = env.observe_tokens(active.tolist())
                empty = active & ~observation_legal(observation).any(1)
                empty_actions |= empty
                capped |= empty
                active &= ~empty
                if not active.any():
                    break
                indices = np.flatnonzero(active)
                rows = pack_batch(observation)
                if len(rows) != count:
                    raise ValueError("packed observation row count mismatch")
                inputs = unpack([rows[index] for index in indices], target, model)
                with torch.inference_mode():
                    logits = predict(model, inputs, args.precision, policy_only=True)
                    selected = logits.masked_fill(~inputs[6], -torch.inf).argmax(1).cpu().numpy()
                choice = np.zeros(count, np.int64); choice[indices] = selected
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
                step_capped |= cap & (steps >= (max_steps or args.max_steps))
                combat_capped |= cap & ~step_capped
                capped |= cap
                active &= ~(done | cap)
            for index, row in enumerate(env.stats()):
                outcomes.append(int(not capped[index] and row[4] == 12))
                floors.append(int((row[0] - 1) * 17 + row[1]))
            caps += int(capped.sum())
            step_caps += int(step_capped.sum())
            combat_caps += int(combat_capped.sum())
            empty_count += int(empty_actions.sum())
        wins = sum(outcome == 1 for outcome in outcomes)
        boss_entries = sum(outcome == 1 or floor >= 51 for outcome, floor in zip(outcomes, floors))
        reports.append({
            "character": character, "runs": total, "resolved": total - caps, "wins": wins,
            "win_rate": wins / total, "win_rate_interval": wilson(wins, total),
            "floor_mean": float(np.mean(floors)),
            "floor_bands": bands(floors), "outcomes": outcomes, "floors": floors,
            "boss_entries": boss_entries, "boss_entry_rate": boss_entries / total,
            "boss_entry_rate_interval": wilson(boss_entries, total),
            "boss_conversion": wins / max(1, boss_entries),
            "boss_conversion_interval": wilson(wins, boss_entries),
            "caps": caps, "step_caps": step_caps, "combat_caps": combat_caps,
            "empty_actions": empty_count,
        })
    if was_training:
        model.train()
    wins = sum(row["wins"] for row in reports)
    total = sum(row["runs"] for row in reports)
    boss_entries = sum(row["boss_entries"] for row in reports)
    return {
        "stage": {"ascension": ascension, "bonus": bonus}, "characters": reports,
        "wins": wins, "runs": total, "win_rate": wins / total,
        "win_rate_interval": wilson(wins, total),
        "floor_mean": float(np.mean([floor for row in reports for floor in row["floors"]])),
        "boss_entries": boss_entries, "boss_entry_rate": boss_entries / total,
        "boss_entry_rate_interval": wilson(boss_entries, total),
        "boss_conversion": wins / max(1, boss_entries),
        "boss_conversion_interval": wilson(wins, boss_entries),
        "caps": sum(row["caps"] for row in reports),
    }


def immutable_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as output:
        json.dump(value, output, indent=2, sort_keys=True)


def atomic_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True))
    temporary.replace(path)


def evaluation_score(result):
    characters = result["characters"]
    return (
        sum(character["wins"] > 0 for character in characters),
        min(character["win_rate_interval"][0] for character in characters),
        result["win_rate"],
        min(character["floor_mean"] - 17 * character["caps"] / character["runs"]
            for character in characters),
        result["floor_mean"],
    )


def seed_panel(seed, runs):
    return seed, seed + 4_000_000 + runs


def link_checkpoint(source, target):
    temporary = target.with_suffix(".tmp")
    temporary.unlink(missing_ok=True)
    os.link(source, temporary)
    temporary.replace(target)


def sha256_file(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def load_stage_bests(output, development, checkpoints, best):
    stages = {
        key: row for key, row in best.get("stages", {}).items()
        if (output / row["checkpoint"]).exists()
    }
    for checkpoint in checkpoints.glob("*.pt"):
        development_path = development / f"{int(checkpoint.stem):012}.json"
        if not development_path.exists():
            continue
        evaluation = json.loads(development_path.read_text())
        key = str(evaluation["stage"]["index"])
        record = {
            "step": evaluation["step"], "stage": evaluation["stage"],
            "score": evaluation["score"], "development": evaluation["result"],
            "checkpoint": str(checkpoint.relative_to(output)),
        }
        if tuple(record["score"]) > tuple(stages.get(key, {}).get("score", ())):
            record["sha256"] = (best["sha256"]
                                if best.get("checkpoint") == record["checkpoint"] and best.get("sha256")
                                else sha256_file(checkpoint))
            stages[key] = record
    return stages


def save_checkpoint(path, model, optimizer, manifest, stage, decisions, sampler_session,
                    promotion_index, progress_active, reservoir, auxiliary_decisions,
                    stage_decisions, replace=False):
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
            "stage_decisions": stage_decisions,
            "winning_reservoir": reservoir.state_dict() if reservoir else None,
            "model": model.state_dict(), "optimizer": optimizer.state_dict(),
            "torch_rng": torch.get_rng_state(),
        }, output)
    if replace:
        target.replace(path)
    return sha256_file(path)


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
    for path in target.rglob("run.json"):
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
            continue
        name = str(run.relative_to(target))
        try:
            best = json.loads((run / "best.json").read_text())
        except (OSError, json.JSONDecodeError):
            best = None
        report_rows = [
            {"iteration": reports[key]["metrics"].get("iteration", index),
             "step": reports[key]["step"], "stage": reports[key].get("stage", {}),
             "description": reports[key].get("description", ""),
             "pipeline": reports[key].get("pipeline", []),
             "metrics": reports[key]["metrics"], "_written": reports[key].get("_written")}
            for index, key in enumerate(sorted(reports), 1)
        ]
        version_groups = {}
        for row in report_rows:
            marker = row["description"].partition("Continuous V")[2].partition(" ")[0]
            version = int(marker) if marker.isdigit() else manifest.get("model_version", manifest.get("version", 0))
            version_groups.setdefault(version, []).append(row)
        for version, version_reports in version_groups.items():
            version_manifest = manifest | manifest.get("version_history", {}).get(str(version), {})
            version_manifest["model_version"] = version
            if version == MODEL_VERSION:
                current = Agent(version_manifest["layout"])
                version_manifest |= {
                    "architecture": architecture(current), "change": CHANGE,
                    "feature_version": FEATURE_VERSION,
                    "parameters": sum(parameter.numel() for parameter in current.parameters()),
                }
            if len(version_groups) > 1:
                sessions = [row for row in manifest.get("sessions", []) if row["step"] <= version_reports[-1]["step"]]
                version_manifest["sessions"] = sessions[-1:] or manifest.get("sessions", [])[:1]
            low, high = version_reports[0]["step"], version_reports[-1]["step"]
            runs[name if len(version_groups) == 1 else f"{name}/V{version}"] = {
                "version": version, "manifest": version_manifest,
                "source": manifest.get("source") if len(version_groups) == 1 else None,
                "reports": version_reports,
                "promotions": [promotions[key] for key in sorted(promotions) if low <= key <= high],
                "best": best if version == manifest.get("model_version") else None,
            }
    def history(name, seen=()):
        row = runs[name]
        source = Path(row["source"]).resolve() if row["source"] else None
        parents = [candidate for candidate in runs if candidate not in seen + (name,)
                   and source and source.is_relative_to((target / candidate).resolve())]
        compatible = lambda candidate: runs[candidate]["version"] == row["version"] and runs[candidate]["manifest"].get("architecture") == row["manifest"].get("architecture")
        parents = [candidate for candidate in parents if compatible(candidate)]
        start = min((int(session["step"]) for session in row["manifest"].get("sessions", []) if "step" in session), default=None)
        if not parents and source and start is not None:
            checkpoint = f"{start:012}.pt"
            parents = [candidate for candidate in runs if candidate not in seen + (name,) and compatible(candidate) and (target / candidate / "checkpoints" / checkpoint).exists()]
        parent = max(parents, key=lambda candidate: runs[candidate]["reports"][-1]["step"], default=None)
        inherited = [report for report in history(parent, seen + (name,)) if start is None or report["step"] <= start] if parent else []
        return list({report["step"]: report for report in inherited + row["reports"]}.values())
    histories = {name: sorted(history(name), key=lambda report: report["step"]) for name in runs}
    for name in runs:
        runs[name]["reports"] = histories[name]
        runs[name].pop("source")
    runs = dict(sorted(runs.items(), key=lambda item: (item[1]["version"], item[1]["reports"][-1]["step"], item[0])))
    data = json.dumps({name: run | {"run": name} for name, run in runs.items()}, separators=(",", ":")).replace("</", "<\\/")
    content = """<!doctype html><meta charset=utf-8><title>Spirefysh dashboard</title><script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script><style>
body{font:14px system-ui;margin:24px;background:#101319;color:#e8ecf2}h1,h2{margin-bottom:6px}.controls,.pipeline{display:flex;align-items:center;gap:12px;flex-wrap:wrap;margin-bottom:22px}select,input{padding:7px;background:#202938;color:#e8ecf2;border:1px solid #526176;border-radius:5px}.charts{display:grid;grid-template-columns:repeat(auto-fit,minmax(440px,1fr));gap:14px}.panel{margin:22px 0;padding:16px;background:#171d28;border:1px solid #303a49;border-radius:10px}.charts .panel{margin:0}.plot{height:340px;min-width:0}.pipeline span{padding:10px;background:#202938;border-radius:6px}.pipeline span+span:before{content:'→';margin-right:12px;color:#8794a8}table{border-collapse:collapse;width:100%}th,td{padding:7px;border-bottom:1px solid #303a49;text-align:left}.yes{color:#75db91}.no{color:#ee7b7b}.state-flow{display:grid;grid-template-columns:1fr auto 1fr auto 1fr auto 1.2fr;gap:10px;align-items:stretch;margin:14px 0}.state-flow.action-flow{grid-template-columns:1fr auto 1fr auto 1fr}.state-node{padding:14px;border:1px solid #526176;border-radius:8px;background:#202938}.state-node b{display:block;font-size:18px;margin-bottom:5px}.state-node[data-tip]{cursor:help}.module-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(210px,1fr));gap:10px;margin:14px 0}.module-grid .state-node{min-height:68px}.state-plus{align-self:center;color:#8794a8;font-size:24px}.state-inspectors,.latent-columns{display:grid;grid-template-columns:repeat(auto-fit,minmax(440px,1fr));gap:14px}.state-box{padding:14px;background:#121824;border:1px solid #303a49;border-radius:8px}.state-box summary{cursor:pointer;font-weight:700;margin-bottom:12px}.training-diagrams{display:grid;gap:14px}.training-diagram{padding:14px;background:#121824;border:1px solid #303a49;border-radius:8px}.training-diagram h3{margin:0 0 10px}.flow-row{display:flex;align-items:stretch;gap:8px;overflow-x:auto;padding:3px}.flow-node{flex:1;min-width:165px;padding:12px;border:1px solid #526176;border-top:3px solid var(--cell);border-radius:7px;background:#202938;cursor:help}.flow-node b{display:block;margin-bottom:5px}.flow-node small{display:block;color:#aeb8c8;line-height:1.35}.flow-arrow{align-self:center;color:#8794a8;font-size:22px}.float-group{margin:12px 0}.float-group h4,.latent-row h4{margin:4px 0;color:#b9c7d8}.float-grid{display:grid;grid-template-columns:repeat(32,11px);gap:2px}.float-grid.token{grid-template-columns:repeat(8,42px);gap:5px}.float-grid.latent{grid-template-columns:repeat(64,7px);gap:2px}.float{width:11px;height:11px;padding:0;border:1px solid color-mix(in srgb,var(--cell) 75%,#fff 10%);border-radius:2px;background:color-mix(in srgb,var(--cell) 45%,#101319);cursor:help}.float.token{width:42px;height:28px;color:#e8ecf2;font-size:10px}.float.latent{width:7px;height:7px}.float:hover,.float:focus,.state-node[data-tip]:hover,.state-node[data-tip]:focus,.flow-node:hover,.flow-node:focus{outline:2px solid #fff;position:relative;z-index:2}.latent-row{margin:10px 0}.state-tip{position:fixed;display:none;z-index:1000;max-width:520px;padding:10px 12px;border:1px solid #6fb1ff;border-radius:7px;background:#080b10;color:#e8ecf2;white-space:pre-wrap;box-shadow:0 8px 28px #000b;pointer-events:none}.state-note{color:#aeb8c8}.unsupported{padding:18px;border:1px dashed #526176;border-radius:8px;color:#aeb8c8}
</style><h1 id=title>Spirefysh dashboard</h1><div class=controls><label>Run <select id=version></select></label><label>X axis <select id=xaxis><option value=iteration>Iteration</option><option value=decisions selected># decisions</option><option value=time>Wall-clock time</option></select></label><label><input id=smooth type=checkbox checked> EMA</label><label>EMA α <input id=ema type=number min=.01 max=1 step=.01 value=.2></label><span>Auto-refresh 15s</span></div><section class=panel><h2>Selected run</h2><div id=details></div></section><div class=charts><section class=panel><h2>Mean advantage</h2><div id=advantage class=plot></div></section><section class=panel><h2>Decisions / second</h2><div id=throughput class=plot></div></section><section class=panel><h2>Terminal floor</h2><div id=floor class=plot></div></section><section class=panel><h2>Ascension</h2><div id=ascension class=plot></div></section><section class=panel><h2>Bonus strength</h2><div id=bonus class=plot></div></section><section class=panel><h2>Win proportion</h2><div id=wins class=plot></div></section><section class=panel><h2>Clip fraction</h2><div id=clip class=plot></div></section><section class=panel><h2>KL</h2><div id=kl class=plot></div></section><section class=panel><h2>Entropy</h2><div id=entropy class=plot></div></section></div><section class=panel><h2>Promotion</h2><div id=promotion></div></section><section class=panel><h2>State representation</h2><p class=state-note>Hover or focus any element for its shape and contents.</p><div id=stateDiagram></div></section><section class=panel><h2>Training pipeline</h2><p class=state-note>Hover or focus any stage for its exact rule, limit, and latest measured value.</p><div id=trainingDiagram></div></section><div id=stateTip class=state-tip></div><script>const versions=""" + data + r""",versionSelect=document.querySelector('#version'),xaxis=document.querySelector('#xaxis'),smooth=document.querySelector('#smooth'),ema=document.querySelector('#ema');
const characterNames=['Ironclad','Defect','Silent','Regent','Necrobinder'],characterColors=['#ef4444','#38bdf8','#22c55e','#f59e0b','#a78bfa'],names=Object.keys(versions),config={responsive:true,displaylogo:false},refreshKey='spirefysh-dashboard',saved=(()=>{try{return JSON.parse(sessionStorage.getItem(refreshKey))}catch{return null}})();let timeOrigin=0;versionSelect.innerHTML=names.map(name=>`<option value="${name}">V${versions[name].version} · ${name}</option>`).join('');versionSelect.value=saved?.followLatest?names.at(-1):names.includes(saved?.version)?saved.version:names.at(-1)||'';if(saved?.xaxis)xaxis.value=saved.xaxis;if(typeof saved?.smooth==='boolean')smooth.checked=saved.smooth;if(saved?.ema)ema.value=saved.ema;
function x(report){return xaxis.value==='time'?(Number.isFinite(Number(report._written))?(report._written-timeOrigin)/60:(report.metrics.seconds||0)/60):xaxis.value==='decisions'?report.step:report.iteration}function series(history,key){return history.map(report=>({x:x(report),y:Number(report.metrics[key])})).filter(point=>Number.isFinite(point.y))}
function emaLine(points){const alpha=Math.max(.01,Math.min(1,Number(ema.value)||.2));let value;return points.map((point,index)=>({x:point.x,y:value=index?alpha*point.y+(1-alpha)*value:point.y}))}
function stageTransitions(history,run){const promotions=(run?.promotions||[]).filter(row=>row.promoted);if(promotions.length)return promotions.map(promotion=>{const report=history.find(row=>row.step===promotion.step)||history.filter(row=>row.step<=promotion.step).at(-1),after=history.find(row=>row.step>promotion.step),next=run.manifest.stages?.[(promotion.stage?.index??-1)+1]||after?.stage,position=xaxis.value==='decisions'?promotion.step:report&&xaxis.value==='iteration'?report.metrics.iteration:Number.isFinite(Number(promotion._written))?(promotion._written-timeOrigin)/60:Number.isFinite(Number(promotion.seconds))?promotion.seconds/60:report?x(report):NaN;return{x:position,stage:next}}).filter(point=>Number.isFinite(Number(point.x)));return history.slice(1).flatMap((row,index)=>Number.isFinite(Number(row.stage?.ascension))&&Number.isFinite(Number(history[index].stage?.ascension))&&(row.stage.ascension!==history[index].stage.ascension||row.stage.bonus!==history[index].stage.bonus)?[{x:x(row),stage:row.stage}]:[])}
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
const cardNumeric=[['Upgrades','card.upgrades'],['Enchantment amount','card.enchantment_amount'],['Enchantment value','card.enchantment_value'],['Variant','card.variant'],['Reserved','flags use categorical field 8'],['Reserved','turn flags use categorical field 9'],['Card value','card.value'],['Replay count','card.replays'],['Free flag','float(card.free)'],['Printed energy cost','card definition cost at upgrade'],['Printed star cost','card definition star cost at upgrade'],['Cost delta','card.cost_delta'],['Cost override','0 or card.cost_override + 129'],['Master-card relation','master instance/index relation to run deck'],['Related upgrades','upgrades on related master card'],['Context','zone-specific price, bundle or delayed context'],['Effective energy cost','current computed energy cost after effects'],['Effective star cost','current computed star cost'],['Preview block','deterministic card preview block'],['Chosen-target damage','sum preview damage for chosen/random/lowest targets'],['All-target damage','sum preview damage for all/other targets'],['Preview draw','preview.draw'],['Preview discard','preview.discard'],['Preview exhaust','preview.exhaust']];
const legacyCardZones=['Run deck','Combat hand','Draw pile','Discard pile','Exhaust pile','Combat offer pile','Reward/shop/choice/bundle offers','History Course card',"Pael's stored cards",'Slippery Bridge selected card','Slippery Bridge candidates','Currently playing card','Queued automatic plays'],cardZones=['Run deck','Combat hand','Draw pile','Discard pile','Exhaust pile','History Course card',"Pael's stored cards",'Slippery Bridge previous cards','Currently playing card','Queued automatic plays'];
const entityNames=['Enemies','Powers','Delayed effects','Relics','Potions','Continuations','Map','Crystal board','Encounters'];
function globalFields(run){const fields=[],add=(group,name,source)=>fields.push({group,name,source});scalarBase.forEach(([name,source])=>add('Scalar bank',name,source));combatScalar.forEach(([name,source])=>add('Scalar bank',name,source));add('Scalar bank','Reserved scalar 116','Currently zero; reserved phase-local scalar slot');add('Scalar bank','Reserved scalar 117','Currently zero; reserved phase-local scalar slot');for(let i=0;i<10;i++)add('Scalar bank',`Reward/Tinker summary ${i}`,i<8?`Rewards: card reward ${Math.floor(i/2)} ${i%2?'detail':'kind'}; Tinker Time: ${i?'offer '+i+' present':'event_data[3] / 3'}`:`Rewards: zero; Tinker Time: offer ${i} present`);for(let i=0;i<5;i++)add('Character one-hot',`Character ${i}`,`1 when run.character == ${i}, else 0`);for(let i=0;i<5;i++)add('Act one-hot',`Act ${i}`,`1 when game.act == ${i}, else 0`);for(let i=0;i<14;i++)add('Phase one-hot',`Phase ${i}`,`1 when phase_index(game.phase) == ${i}, else 0`);for(let i=0;i<8;i++)add('Room one-hot',`Room ${i}`,`1 when room_index(game.room) == ${i}, else 0`);for(let i=0;i<66;i++)add('Current event',`Event ID ${i}`,`1 when current phase is Event(${i}), else 0`);for(let i=0;i<66;i++)add('Visited events',`Visited event ID ${i}`,`1 when game.visited_events contains ${i}, else 0`);for(let i=0;i<4;i++)add('Event requirements',`Requirement ${i}`,`accumulated event option requirement feature ${i}`);for(let i=0;i<36;i++)add('Event effects',`Effect ${i}`,`accumulated event option effect feature ${i}`);for(let i=0;i<8;i++)add('Event values',`Event value ${i}`,`normalized event option numeric value ${i}`);for(let i=0;i<4;i++)add('Event-local state',`Event state ${i}`,`game.event_data[${i}] / 1000 for supported events, else 0`);for(let i=0;i<22;i++)add('Enchant choice',`Enchantment ${i}`,`1 when EnchantCards selects enchantment ${i}, else 0`);const globals=run.manifest.layout.globals,orbSlots=(globals-400)/6;if(!Number.isInteger(orbSlots)||orbSlots<0)return[];for(let slot=0;slot<orbSlots;slot++){for(let id=0;id<5;id++)add('Orb slots',`Orb slot ${slot}: type ${id}`,`1 when combat.orbs[${slot}].id == ${id}, else 0`);add('Orb slots',`Orb slot ${slot}: value`,`combat.orbs[${slot}].value / 100; zero when slot is empty`)}[['Osty maximum HP','combat.osty.max_hp / 500'],['Burning Sticks','float(combat.burning_sticks)'],['Throwing Axe','float(combat.throwing_axe)'],["Pael's Eye","float(combat.paels_eye)"],["Pael's Eye extra","float(combat.paels_eye_extra)"],["Pael's Legion",'combat.paels_legion / 2'],['Known draw top','combat.known_draw_top / 255'],['Known draw bottom','combat.known_draw_bottom / 255'],['Nightmare count','combat.nightmares.len / 16'],['Bomb count','combat.bombs.len / 16'],['Automation count','combat.automation.len / 16'],['Panache count','combat.panache.len / 16'],['Boulder count','combat.boulders.len / 16'],['Dampened count','combat.dampened.len / 16']].forEach(([name,source])=>add('Combat summary',name,source));[['Pending curse','float(game.pending_curse)'],['Wongo active','float(game.wongo_combats.is_some())'],['Wongo combats','game.wongo_combats.unwrap_or(0) / 5'],['Golden Compass current act','float(game.golden_compass == run.act)'],['Astrolabe','float(game.astrolabe)'],['Transform niche','float(game.transform_niche)'],["Pael's Tooth","float(game.paels_tooth)"],['Parasol removal','float(game.parasol_removal)'],['Conveyor','float(game.conveyor)'],['Fake shop','float(game.fake_shop)']].forEach(([name,source])=>add('Run-state flags',name,source));[['Choice pile','(pile_index(choice.pile) + 1) / 5'],['Choice filter','filter_features(choice.filter).kind / 15'],['Choice filter value','filter_features(choice.filter).value / 1,000,000'],['Choice operation','op_features(choice.op).kind / 17'],['Choice operation value','op_features(choice.op).value / 1,000,000'],['Choices remaining','choice.remaining / 10'],['Choice/combat flags','bit-pack optional, enemy_turn, ending and force_end; divide by 15'],['Resume phase','(phase_index(game.resume) + 1) / 14; zero without resume']].forEach(([name,source])=>add('Decision context',name,source));add('Training bonuses','Strength bonus','training_strength / 32');add('Training bonuses','Dexterity bonus','training_dexterity / 32');return fields}
function tokenFields(mode){const unused=()=>Array.from({length:24},(_,i)=>[`Numeric ${i}`,'Unused by this token kind; emitted as 0']),spec={card:[10,'State card zone: 1…13 through V36; 1…10 from V37','card.id + 1','1 for state cards','known draw position or 0','card type + 1','enchantment + 1','1 Status, 2 Curse, 3 Quest, else 0',cardNumeric],enemy:[1,'0 live enemy, 1 move history, 2 public Wriggler parity','enemy ID + 1 or prior move + 1','enemy slot for history tokens','enemy slot or history position','0','0','0',unused()],power:[2,'0 current, 1 snapshot','power.id + 1','1 player, 2 Osty, 3+ enemy slot','power list position + 1','0','0','0',unused()],delayed:[3,'17 Nightmare card, 18 dampened card, 2 bomb, 3 automation, 4 panache, 5 boulder, 6 lamp','card/item ID where applicable','card zone/owner where applicable','item position + 1','card type for card variants','enchantment for card variants','affliction for card variants',cardNumeric.map(([n,s])=>[n,`Semantic-card variants: ${s}; otherwise kind-specific or 0`])],relic:[4,'0 owned; 1/2 bags; 3 reward; 4 Parasol; 5 fake merchant; 6 reward; 7 shop; 8–10 trades; 16 ancient offer','relic.id + 1','rarity or event-option owner','inventory/offer position + 1','0','0','0',unused()],potion:[5,'0 slot; 1 pending; 2 reward; 3 shop; 4 Parasol','potion.id + 1, or 0 for empty slot','0','slot/offer position + 1','0','0','0',unused()],continuation:[6,'2 removal; 3 reward gold; 4 requirement; 5/7/8/9 exact effects; 6 conveyor or Parasol card; 10–17 resume; 18 context; 19 queued card; 20 resume card; 21 exact payload','effect type/card ID where applicable','effect kind, phase owner or item position','effect/item position','card type for card variants','enchantment for card variants','affliction for card variants',cardNumeric.map(([n,s])=>[n,`Semantic-card variants: ${s}; otherwise continuation-kind-specific or 0`])],map:[7,'0 node, 1 directed edge','room + 1 for node; destination lane + 1 for edge','node floor','node lane + 1','0','0','0',unused()],crystal:[8,'0 crystal cell','visible item + 1 or 0','row + 1','column + 1','0','0','0',unused()],encounter:[9,'0 boss; 1 normal pool; 2 elite pool; 3 last normal; 4 last elite','index + 1 in the corresponding encounter vocabulary','0','0','0','0','0',unused()] }[mode];const [collection,kind,id,owner,position,type,enchantment,affliction,numeric]=spec;if(mode==='enemy'){numeric[0]=['Wriggler parity','public enemy-slot parity for Wriggler, else 0'];numeric[2]=['Enemy HP','enemy.hp / 500'];numeric[3]=['Enemy maximum HP','enemy.max_hp / 500'];numeric[4]=['Enemy block','enemy.block / 100'];numeric[5]=['Current move','enemy.move_index / 20'];numeric[6]=['Previous move','enemy.last_move / 20, or -1'];numeric[7]=['Repeat count','enemy.repeats / 10'];numeric[8]=['Stunned','float(enemy.stunned)'];numeric[9]=['Enemy value','enemy.value / 100'];numeric[10]=['Predicted hit','combat.hits[slot] / 20']}if(mode==='power'){numeric[0]=['Power amount','power.amount / 32768'];numeric[1]=['Power value','power.value / 32768'];numeric[2]=['Skip next decay','float(power.skip_next_decay)']}if(mode==='delayed'){numeric[0]=['Primary value','Card upgrades; bomb turns/3; automation turns/10; panache cards/5; boulder amount/100'];numeric[1]=['Secondary value','Card enchantment amount; bomb amount/100; automation energy/10; panache start/20']}if(mode==='relic'){numeric[0]=['Relic state / price','Owned: melted flag; Parasol/shop: price / 500'];numeric[1]=['Wax priority','Owned relic wax priority, else 0']}if(mode==='potion')numeric[0]=['Potion price','Parasol/shop price / 500, else 0'];if(mode==='continuation'){numeric[0]=['Primary continuation value','Card upgrades; payload byte/255; payload length/16384; removal price/500; reward gold/500; conveyor value/1000'];numeric[1]=['Secondary continuation value','Card enchantment amount or exact payload byte/255']}if(mode==='map'){numeric[0]=['Current node','1 when this node is current'];numeric[1]=['Fur Coat node','1 when marked by Fur Coat in current act'];numeric[2]=['Spoils node','1 when marked for spoils']}if(mode==='crystal')numeric[0]=['Cleared cell','float(clear)'];return[['Collection',`constant ${collection}: ${mode}`],['Kind / zone',kind],['ID',id],['Owner',owner],['Position',position],['Card type',type],['Enchantment',enchantment],['Affliction',affliction],...numeric]}
const stateColors={'Scalar bank':'#38bdf8','Character one-hot':'#60a5fa','Act one-hot':'#818cf8','Phase one-hot':'#a78bfa','Room one-hot':'#c084fc','Current event':'#e879f9','Visited events':'#f472b6','Event requirements':'#fb7185','Event effects':'#f97316','Event values':'#f59e0b','Event-local state':'#eab308','Enchant choice':'#84cc16','Orb slots':'#22c55e','Combat summary':'#14b8a6','Run-state flags':'#06b6d4','Decision context':'#0ea5e9','Training bonuses':'#f8fafc'};
const escapeAttribute=value=>String(value).replaceAll('&','&amp;').replaceAll('"','&quot;').replaceAll('<','&lt;').replaceAll('>','&gt;');
function stateCell(index,tip,color,kind=''){return `<button class="float ${kind}" style="--cell:${color}" data-tip="${escapeAttribute(tip)}" aria-label="${escapeAttribute(tip)}" tabindex="0">${kind==='token'?index:''}</button>`}
function renderToken(mode){const fields=tokenFields(mode);document.querySelector('#tokenGrid').innerHTML=fields.map(([name,source],index)=>stateCell(index,`token[${index}] · ${name}\nSource: ${source}`,'#a78bfa','token')).join('')}
function latentRows(start,names,color,kind,width){return names.map((name,group)=>`<div class=latent-row><h4>${name} · state[${start+group*width}…${start+(group+1)*width-1}]</h4><div class="float-grid latent">${Array.from({length:width},(_,latent)=>stateCell(latent,`state[${start+group*width+latent}]\n${name} · latent dimension ${latent}\nSource: ${kind}. This is a learned mixture, not a one-to-one raw feature.`,color,'latent')).join('')}</div></div>`).join('')}
function renderUnifiedState(run){const target=document.querySelector('#stateDiagram'),manifest=run.manifest,layout=manifest.layout,width=manifest.architecture.width,layers=manifest.architecture.layers,decisionLayers=manifest.architecture.action_layers,feedforward=manifest.architecture.feedforward,transformer=4*width*width+2*width*feedforward+9*width+feedforward,rows=[layout.collections,layout.kinds,layout.ids,layout.owners,layout.positions,layout.card_types,layout.enchantments,layout.afflictions].reduce((sum,size)=>sum+Math.min(size,257)+Math.max(2,Math.ceil((size-1)/256)+1),0),shared=rows*width+layout.numeric*width+2*width,compact=manifest.feature_version>=39,semantic=compact?3*width*width+(2*layout.powers+layout.relics+layout.relic_set+layout.event_set+layout.encounter_set+8)*width:0,map=compact?6*width*width+11*width:0,state=width+layers*transformer,action=(layout.action_values+3)*width,decision=width+decisionLayers*transformer,head=width*width+2*width+1,total=shared+semantic+map+state+action+decision+3*head,n=value=>value.toLocaleString();target.innerHTML=`<div class=state-flow><div class=state-node><b>Typed state tokens</b>${compact?'semantic state, per-relic and per-power state, folded histories/effects and three set summaries':'run, combat, entities, relations and public continuation'}</div><div class=state-plus>→</div>${compact?`<div class=state-node><b>${width}</b>current-node map token · one map Transformer layer</div><div class=state-plus>→</div>`:''}<div class=state-node><b>${width}</b>state token · ${layers} Transformer layers</div></div><div class="state-flow action-flow"><div class=state-node><b>Candidate payloads</b>explicit presence and legality per candidate</div><div class=state-plus>→</div><div class=state-node><b>${width}</b>token per candidate</div><div class=state-plus>→</div><div class=state-node><b>${width}</b>decision token · ${decisionLayers} Transformer layers</div></div><div class=module-grid><div class=state-node><b>${n(shared)} params</b>shared token encoder</div>${compact?`<div class=state-node><b>${n(semantic)} params</b>semantic/set/effect encoders</div><div class=state-node><b>${n(map)} params</b>map encoder</div>`:''}<div class=state-node><b>${n(state)} params</b>state Transformer</div><div class=state-node><b>${n(action)} params</b>candidate encoder</div><div class=state-node><b>${n(decision)} params</b>decision Transformer</div><div class=state-node><b>${n(head)} params</b>policy head</div><div class=state-node><b>${n(head)} params</b>win-value head</div><div class=state-node><b>${n(head)} params</b>progress-value head</div><div class=state-node><b>${n(total)} params</b>complete model · manifest ${n(manifest.parameters)}</div></div><details class=state-box open><summary>Representation contract</summary><p>${compact?'Remaining relics, events and encounters each contribute one learned summary token. Owned and excluded relics remain individually identifiable; relic counters live on their relic token. Enemy move history, Pael’s Tooth cards and effect-card context are encoded before the main state Transformer. The map Transformer emits only the current-node token.':'There is no dense model-state vector and no learned card-zone slots. Unordered facts are permutation invariant; meaningful order is carried by token fields.'} Offers exist only as candidate actions. Automatic effects resolve without a model call; choices that interrupt resolution receive typed continuation tokens.</p></details>`}
function renderState(run){const target=document.querySelector('#stateDiagram'),manifest=run.manifest,feature=manifest.feature_version,globals=manifest.layout?.globals,width=manifest.architecture?.width||128,zones=manifest.architecture?.card_zones||13;if(feature<33||![460,592].includes(globals)||width!==128||![10,13].includes(zones)){target.innerHTML=`<div class=unsupported>Detailed float provenance is unavailable for feature ${feature}, globals ${globals}, width ${width}, zones ${zones}.</div>`;return}const fields=globalFields(run);if(fields.length!==globals){target.innerHTML=`<div class=unsupported>State metadata mismatch: expected ${globals} global floats, mapped ${fields.length}.</div>`;return}const grouped=new Map;fields.forEach((field,index)=>{if(!grouped.has(field.group))grouped.set(field.group,[]);grouped.get(field.group).push([field,index])});const layout=manifest.layout,layers=manifest.architecture.layers,actionLayers=manifest.architecture.action_layers||0,feedforward=manifest.architecture.feedforward,collections=layout.collections-2,transformer=4*width*width+2*width*feedforward+9*width+feedforward,rows=[layout.collections,layout.kinds,layout.ids,layout.owners,layout.positions,layout.card_types,layout.enchantments,layout.afflictions].reduce((sum,size)=>sum+Math.min(size,257)+Math.max(2,Math.ceil((size-1)/256)+1),0),shared=rows*width+layout.numeric*width+2*width,card=zones*width+layers*transformer,entity=(collections+1)*width,cardStart=globals,entityStart=globals+zones*width,baseState=entityStart+(collections-1)*width,decisionState=baseState+(actionLayers?width:0),action=(layout.action_values+3)*width,actionSet=actionLayers?width+actionLayers*transformer:0,policy=(decisionState+width)*width+2*width+1,value=decisionState*width+2*width+1,total=shared+card+entity+action+actionSet+policy+2*value,number=value=>value.toLocaleString(),tip=(title,formula)=>escapeAttribute(`${title}\n${formula}`),zoneNames=zones===13?legacyCardZones:cardZones,globalHtml=[...grouped].map(([group,rows])=>`<div class=float-group><h4>${group} · ${rows.length}</h4><div class=float-grid>${rows.map(([field,index])=>stateCell(index,`state[${index}] = global[${index}]\n${field.group} · ${field.name}\nSource: ${field.source}`,stateColors[group])).join('')}</div></div>`).join('');target.innerHTML=`<div class=state-flow><div class=state-node data-tip="${tip('Direct global vector',`${globals} raw values; no learned parameters`)}" tabindex=0><b>${globals}</b>direct global floats · 0 params</div><div class=state-plus>+</div><div class=state-node data-tip="${tip('Card-zone state',`${zones} zones × ${width}; learned by the shared token encoder and card Transformer`)}" tabindex=0><b>${zones} × ${width}</b>card-zone outputs</div><div class=state-plus>+</div><div class=state-node data-tip="${tip('Non-card state',`${collections-1} collections × ${width}; learned by shared token encoder and entity pooling`)}" tabindex=0><b>${collections-1} × ${width}</b>entity-pool outputs</div><div class=state-plus>=</div><div class=state-node data-tip="${tip('Base state vector',`${globals} + ${zones*width} + ${(collections-1)*width} = ${baseState} floats`)}" tabindex=0><b>${number(baseState)}</b>${actionLayers?'base':'final'} state floats</div></div>${actionLayers?`<div class="state-flow action-flow"><div class=state-node data-tip="${tip('Base state',`${baseState} floats before the represented action set is summarized`)}" tabindex=0><b>${number(baseState)}</b>base state</div><div class=state-plus>+</div><div class=state-node data-tip="${tip('Action-set context',`learned ${width}-d CLS output from ${actionLayers} Transformer layer over all represented actions`)}" tabindex=0><b>${width}</b>action-set context</div><div class=state-plus>=</div><div class=state-node data-tip="${tip('Decision state',`${baseState} + ${width} = ${decisionState} floats supplied to both value heads and the policy`)}" tabindex=0><b>${number(decisionState)}</b>decision-state floats</div></div>`:''}<div class=module-grid><div class=state-node data-tip="${tip('Shared token encoder',`${rows} embedding rows × ${width} + ${layout.numeric}×${width} numeric projection + ${2*width} LayerNorm = ${number(shared)}`)}" tabindex=0><b>${number(shared)} params</b>shared token encoder · 32→${width}</div><div class=state-node data-tip="${tip('Card-zone Transformer',`${zones*width} zone seeds + ${layers} × ${number(transformer)} Transformer-layer params = ${number(card)}`)}" tabindex=0><b>${number(card)} params</b>card-zone Transformer</div><div class=state-node data-tip="${tip('Entity pooling',`${(collections-1)*width} collection states + ${2*width} shared LayerNorm = ${number(entity)}`)}" tabindex=0><b>${number(entity)} params</b>entity pooling</div><div class=state-node data-tip="${tip('Action input encoder',`${layout.action_values}×${width} value projection + ${width} bias + ${2*width} LayerNorm = ${number(action)}; action tokens reuse the shared token encoder`)}" tabindex=0><b>${number(action)} params</b>action input encoder · ${layout.action_values}+tokens→${width}</div>${actionLayers?`<div class=state-node data-tip="${tip('Action-set Transformer',`${width} learned CLS state + ${actionLayers} × ${number(transformer)} Transformer-layer params = ${number(actionSet)}`)}" tabindex=0><b>${number(actionSet)} params</b>action-set Transformer · ${actionLayers} layer</div>`:''}<div class=state-node data-tip="${tip('Policy head',`(${decisionState}+${width})×${width} hidden layer + biases + ${width}→1 score = ${number(policy)}`)}" tabindex=0><b>${number(policy)} params</b>policy head · state+action→1</div><div class=state-node data-tip="${tip('Win-value head',`${decisionState}×${width} hidden layer + biases + ${width}→1 = ${number(value)}`)}" tabindex=0><b>${number(value)} params</b>win-value head · state→1</div><div class=state-node data-tip="${tip('Progress-value head',`${decisionState}×${width} hidden layer + biases + ${width}→1 = ${number(value)}`)}" tabindex=0><b>${number(value)} params</b>progress-value head · state→1</div><div class=state-node data-tip="${tip('Whole model',`calculated ${number(total)} parameters; manifest records ${number(manifest.parameters)}`)}" tabindex=0><b>${number(total)} params</b>complete model</div></div><div class=state-inspectors><details class=state-box open><summary>Direct global floats — hover any of ${globals}</summary>${globalHtml}</details><details class=state-box open><summary>Raw token fields — hover any of 32</summary><label>Token collection <select id=tokenMode>${[['card','Card'],['enemy','Enemy'],['power','Power'],['delayed','Delayed effect'],['relic','Relic'],['potion','Potion'],['continuation','Continuation'],['map','Map'],['crystal','Crystal'],['encounter','Encounter']].map(([value,label])=>`<option value=${value}>${label}</option>`).join('')}</select></label><div id=tokenGrid class="float-grid token"></div></details></div><details class=state-box open><summary>Learned latent floats — hover any state dimension</summary><p class=state-note>These dimensions are learned mixtures. Their tooltip identifies the exact zone, collection, or action-set coordinate.</p><div class=latent-columns><div><h3>Card-zone state · ${zones*width} floats</h3>${latentRows(cardStart,zoneNames,'#e879f9','Transformer zone-seed output over every encoded card in this zone',width)}</div><div><h3>Non-card entity state · ${(collections-1)*width} floats</h3>${latentRows(entityStart,entityNames,'#34d399','LayerNorm(learned collection state + indexed token sum / sqrt(count))',width)}${actionLayers?`<h3>Action-set context · ${width} floats</h3>${latentRows(baseState,['Action-set summary'],'#f59e0b','Action Transformer CLS output over the full represented action set',width)}`:''}</div></div></details>`;const select=document.querySelector('#tokenMode');select.onchange=()=>renderToken(select.value);renderToken(select.value)}
function renderZoneState(run){const target=document.querySelector('#stateDiagram'),manifest=run.manifest,a=manifest.architecture,n=value=>Number(value).toLocaleString(),entity=a.entity_collections*a.width,cards=a.card_zones*a.width;target.innerHTML=`<div class=state-flow><div class=state-node><b>${n(a.globals)}</b>direct public globals</div><div class=state-plus>+</div><div class=state-node><b>${a.card_zones} × ${a.width}</b>card-zone summaries · shared ${a.layers}-layer Transformer</div><div class=state-plus>+</div><div class=state-node><b>${a.entity_collections} × ${a.width}</b>nonlinear pooled entity collections</div><div class=state-plus>+</div><div class=state-node><b>${a.width}</b>current-node map summary</div><div class=state-plus>=</div><div class=state-node><b>${n(a.state_width)}</b>state</div></div><div class="state-flow action-flow"><div class=state-node><b>Candidate payload</b>scalar features + typed tokens</div><div class=state-plus>→</div><div class=state-node><b>${a.width}</b>action</div><div class=state-plus>+</div><div class=state-node><b>${n(a.state_width)}</b>state</div><div class=state-plus>→</div><div class=state-node><b>${a.head_width}</b>policy hidden</div></div><div class=module-grid><div class=state-node><b>${n(manifest.parameters)}</b>total parameters</div><div class=state-node><b>${n(cards)}</b>card summary floats</div><div class=state-node><b>${n(entity)}</b>entity summary floats</div><div class=state-node><b>2</b>value heads over state only</div></div><details class=state-box open><summary>Representation contract</summary><p>Every visible card remains one token in one of ${a.card_zones} zones. The zones share one Transformer but have distinct learned state tokens. Non-card collections use nonlinear tuple projection, sum/√count pooling, and exact/log counts. Public draw rank, map floor/lane, and continuation branch/path/list/sibling order use bounded role-specific positional embeddings alongside normalized coordinates. The direct globals retain normalized singleton state and the public map grid.</p></details>`}
const renderZoneStateBase=renderZoneState;renderZoneState=run=>{const a=run.manifest.architecture;if(!a.pool_tokens)return renderZoneStateBase(run);const target=document.querySelector('#stateDiagram'),n=value=>Number(value).toLocaleString();target.innerHTML=`<div class=state-flow><div class=state-node><b>${n(a.globals)}</b>direct public globals</div><div class=state-plus>+</div><div class=state-node><b>${a.card_zones} × ${a.width}</b>card-zone summaries</div><div class=state-plus>+</div><div class=state-node><b>${a.entity_collections} × ${a.width}</b>entity summaries</div><div class=state-plus>+</div><div class=state-node><b>${a.pool_tokens} × ${a.width}</b>compact pool summaries</div><div class=state-plus>+</div><div class=state-node><b>${a.width}</b>map summary</div><div class=state-plus>=</div><div class=state-node><b>${n(a.state_width)}</b>state</div></div><div class="state-flow action-flow"><div class=state-node><b>State ${n(a.state_width)}</b></div><div class=state-plus>+</div><div class=state-node><b>Action ${a.width}</b></div><div class=state-plus>→</div><div class=state-node><b>Policy hidden ${a.head_width}</b></div></div><div class=module-grid><div class=state-node><b>${n(run.manifest.parameters)}</b>total parameters</div><div class=state-node><b>${a.layers} × W${a.width}/H${a.heads}</b>shared card Transformer</div><div class=state-node><b>${a.pool_tokens}</b>relic/event/encounter pool families</div><div class=state-node><b>2</b>value heads</div></div>`}
function renderDirectState(run){const target=document.querySelector('#stateDiagram'),a=run.manifest.architecture,n=value=>Number(value).toLocaleString(),types=[
['1 · Run','6 semantic + 36 numeric → 64','Character, room, act, floor, HP, max HP, gold, ascension, base resources, odds and encounter counters.','#38bdf8'],
['2 · Phase','12 semantic + 16 numeric → 64','Current and resumed phase, room, event, choice filter and operation, rewards and public event values.','#38bdf8'],
['3 · Cards','73 semantic + 23 numeric → 64 each','Exact card ID, zone, role, public order, type, rarity, target, upgrades, flags, enchantment, dampening, master relation and effective preview.','#e879f9'],
['4 · Actors','5 semantic + 27 numeric → 64 each','Player, Osty and enemies: identity, slot, HP, block, intent, energy, stars and incoming damage.','#34d399'],
['5 · Powers','5 semantic + 8 numeric → 64 each','Owner, exact power ID, source, amount, value, order and decay state.','#34d399'],
['6 · Combat history','4 semantic + 32 numeric → 64 each','Ordered enemy moves and combat counts for cards, attacks, skills, powers, energy, exhaust, discard and HP loss.','#34d399'],
['7 · Statuses','72 semantic + 17 numeric → 64 each','Damage marker, Nightmare, Bomb, Automation, Panache, Boulder and their associated metadata.','#34d399'],
['8 · Relics','8 semantic + 16 numeric → 64 each','Exact relic ID, rarity, order, counters, cycle, cap, melt/wax state, Lamp card and public pool status.','#f59e0b'],
['9 · Potions','5 semantic + 6 numeric → 64 each','Exact potion ID, inventory slot, empty or pending status and price.','#f59e0b'],
['10 · Orbs','5 semantic + 6 numeric → 64 each','Exact orb ID, slot, value and timing.','#f59e0b'],
['11 · Events','4 semantic + 4 numeric → 64 each','Event identity, available or visited status, current option and public payload.','#f59e0b'],
['12 · Encounters','3 semantic + 4 numeric → 64 each','Visible boss, normal and elite pools, previous encounter and previous elite.','#f59e0b'],
['13 · Crystal cells','5 semantic + 10 numeric → 64 each','Coordinates, visible item, cleared state, remaining cells and crystal size.','#f59e0b'],
['14 · Continuations','80 semantic + 24 numeric → 64 each','Effects, phases, cards, rewards, parent, branch, path, list, order, requirements and visible payload.','#a78bfa'],
['15 · Map nodes','7 semantic + 8 numeric → 64 each','Entry, room or end, floor, lane, outdegree, Fur Coat and Spoils.','#60a5fa'],
['16 · Map edges','2 semantic + 4 numeric → 64 each','Source-to-destination connection and edge multiplicity.','#60a5fa']];
const section=(title,nodes)=>`<div class=training-diagram><h3>${title}</h3>${flowRow(nodes)}</div>`;
target.innerHTML=`<p>All sixteen inputs are independent. They share one ${n(a.semantic_vocab)} × ${a.width} semantic table but have separate field weights, numeric projections and normalization.</p><div class=training-diagrams><div class=training-diagram><h3>Independent encoded inputs</h3><div class=module-grid>${types.map(flowNode).join('')}</div></div>${section('Direct state values',[
['Character','B × 5','Character one-hot values.','#38bdf8'],['Singleton state','B × 720','Twenty-four fixed slots of thirty public values.','#38bdf8'],['Dense map grid','B × 2,880','Eighteen floors × eight lanes × twenty public map values.','#38bdf8'],['Direct globals','B × 3,605','These floats bypass learned encoders.','#f59e0b']])}${section('Cards',[
['Cards','Ncard × 64','Encoded cards plus zone and public-order indices.','#e879f9'],['Thirteen zones','13B ragged sequences','Deck, hand, draw, discard, exhaust, combat offer, general offer, History Course, Pael, two Slippery Bridge zones, playing and autoplay.','#e879f9'],['Card Transformer L1','(Ncard + 13B) × 64','Full W64/H4/FF128 attention inside each zone.','#c084fc'],['Card Transformer L2','B × 13 × 64','Only each learned zone seed queries the final layer.','#a78bfa'],['Card summaries','B × 832','Thirteen 64-dimensional zone summaries.','#f59e0b']])}${section('Actors',[
['Powers, history, statuses','Items × 64','Items are grouped by actor; statuses split into damage, Nightmare, Bomb, Automation, Panache and Boulder.','#34d399'],['Eight child pools','P × 8 × 64','Nonlinear sum/√count pooling with exact and log counts.','#34d399'],['Actor base + children','P × 576','One actor vector plus eight attached summaries.','#14b8a6'],['Contextual actors','P × 64','Linear 576→64, LayerNorm and ReLU.','#14b8a6'],['Actor summary','B × 64','All actors pooled for the state; individual actors remain available to targeted actions.','#f59e0b']])}${section('Other state information',[
['Run, phase and public collections','Items × 64','Run, phase, relics, potions, orbs, events, encounters and crystal cells.','#f59e0b'],['Continuations','Tree nodes × 64','Bottom-up parent/child pooling using branch, path, list and order relations.','#a78bfa'],['Ten summaries','B × 10 × 64','Run, phase, actors, relics, potions, orbs, events, encounters, crystal and continuations.','#f59e0b'],['Entity contribution','B × 640','Ten flattened 64-dimensional summaries.','#f59e0b']])}${section('Map',[
['Map nodes and edges','V × 64 and E × 64','Separately encoded nodes and edges.','#60a5fa'],['Reverse sparse propagation','V × 64','Each parent attends only to actual contextualized children with edge-conditioned K,V.','#60a5fa'],['Contextual nodes','V × 64','Destination nodes are reused by Path actions. The selected current node is computed but omitted from the state.','#38bdf8'],['Global map query','B × 64','A learned query attends over every contextual node.','#38bdf8']])}${section('State and outputs',[
['Concatenate','3,605 + 832 + 640 + 64','No learned state-fusion layer.','#f59e0b'],['State','B × 5,141','Direct globals, card summaries, entity summaries and global map summary.','#f59e0b'],['Action','A × 64','Action fields plus attached cards/items, Path destination, target actor and legality.','#e879f9'],['Policy','A × 5,205 → 112 → 1','One direct score per action; illegal logits are masked before the per-state softmax.','#f472b6'],['Two critics','B × 5,141 → 112 → 1','Independent win-logit and progress heads.','#ef4444']])}</div>`}
function renderHierarchicalState(run){const target=document.querySelector('#stateDiagram'),a=run.manifest.architecture,n=value=>Number(value).toLocaleString();target.innerHTML=`<div class=state-flow><div class=state-node><b>10 categorical + 24 numeric</b>shared W${a.width} token encoder</div><div class=state-plus>→</div><div class=state-node><b>${a.card_zones} card zones</b>shared L${a.layers}/H${a.heads}; final query only</div><div class=state-plus>+</div><div class=state-node><b>Actor hierarchy</b>player · optional Osty · public enemy slots</div><div class=state-plus>+</div><div class=state-node><b>${a.pool_collections} unordered pools</b>nonlinear tuples · sum/√count · exact count</div></div><div class=state-flow><div class=state-node><b>Ordered frames</b>character, singleton state, orbs, choices, rewards, continuations</div><div class=state-plus>+</div><div class=state-node><b>${a.map_floors} map floors</b>[MAP] L${a.map_layers}; final query only</div><div class=state-plus>→</div><div class=state-node><b>h ∈ ℝ${a.width}</b>[STATE] ragged L${a.fusion_layers}; final query only</div></div><div class="state-flow action-flow"><div class=state-node><b>Represented candidates</b>visible legal and illegal · separate legality</div><div class=state-plus>→</div><div class=state-node><b>a ∈ ℝ${a.width}</b>scalar + nonlinear token pool</div><div class=state-plus>→</div><div class=state-node><b>m ∈ ℝ${a.width}</b>one h-conditioned attention query</div></div><div class=module-grid><div class=state-node><b>${n(run.manifest.parameters)}</b>total parameters</div><div class=state-node><b>512→128→1</b>policy on [h,a,h×a,m]</div><div class=state-node><b>256→128</b>shared critic on [h,m]</div><div class=state-node><b>2 tiny heads</b>win BCE · progress MSE</div><div class=state-node><b>0 dense globals</b>five-character vector is bookkeeping only</div></div>`}
const renderLegacyState=renderState;renderState=run=>run.manifest.model_version>=44?renderZoneState(run):run.manifest.feature_version>=37?renderUnifiedState(run):renderLegacyState(run);
const renderPreHierarchyState=renderState;renderState=run=>run.manifest.model_version>=51?renderHierarchicalState(run):renderPreHierarchyState(run);
function renderDomainState(run){const target=document.querySelector('#stateDiagram'),a=run.manifest.architecture,n=value=>Number(value).toLocaleString(),v=run.manifest.model_version;if(v<55){target.innerHTML=`<div class=state-flow><div class=state-node><b>16 exact domains</b>field-specific u32/i32 byte limbs</div><div class=state-plus>→</div><div class=state-node><b>W${a.width}</b>V${v} domain encoders</div><div class=state-plus>→</div><div class=state-node><b>Typed hierarchy</b>card zones · Actor DeepSets · sparse map · continuation rows</div></div><div class="state-flow action-flow"><div class=state-node><b>Visible candidates</b>legal + illegal</div><div class=state-plus>→</div><div class=state-node><b>Independent menus</b>candidate-conditioned policy and value</div></div>`;return}target.innerHTML=`<div class=state-flow><div class=state-node><b>16 typed domains</b>exact u32/i32 replay fields · namespaced semantic IDs · normalized f32 auxiliaries</div><div class=state-plus>→</div><div class=state-node><b>W${a.width}</b>shared semantic table + multiplicative field-role and domain numeric encoders</div><div class=state-plus>→</div><div class=state-node><b>Typed hierarchy</b>10 card zones · ordered powers · Actor DeepSets · bottom-up continuation trees</div></div><div class=state-flow><div class=state-node><b>Sparse map DAG</b>reverse-topological edge-conditioned H${a.heads} update</div><div class=state-plus>→</div><div class=state-node><b>CURRENT or ENTRY + GLOBAL</b>one positional context; Path candidates reuse destination context</div><div class=state-plus>→</div><div class=state-node><b>s ∈ ℝ${a.width}</b>[STATE] ragged L${a.fusion_layers}; final query only</div></div><div class="state-flow action-flow"><div class=state-node><b>Visible candidates</b>legal + illegal · target actor joins · 20 transition/resource auxiliaries</div><div class=state-plus>→</div><div class=state-node><b>a ∈ ℝ${a.width}</b>shared semantic/numeric encoder + payload count/log pooling</div><div class=state-plus>→</div><div class=state-node><b>hπ and hv</b>independent one-query menus over every present candidate</div></div><div class=module-grid><div class=state-node><b>${n(run.manifest.parameters)}</b>total parameters</div><div class=state-node><b>384→128→1</b>policy on [hπ,a,hπ×a]</div><div class=state-node><b>128→1</b>candidate-conditioned win critic on hv</div><div class=state-node><b>128→1</b>training-only progress head on hv</div><div class=state-node><b>0 dense globals</b>exact identity never passes through fp16</div></div><details class=state-box open><summary>Representation contract</summary><p>One shared CardId namespace is reused in state, status, continuation, and candidate rows. Multiplicative field-role scales prevent same-namespace slot swaps from aliasing. Unordered collections use nonlinear sum/√count pooling with count and log-count; continuation payloads are combined bottom-up through explicit parent, branch, path, list, order, arity, and empty relations. Actor targets and map destinations are gathered into candidates before both menus. Candidate outcomes include cost, resource-after values, damage ratio, lethal, signed HP-after/overkill, player HP, block, energy, stars, draw, discard, and exhaust deltas. Both menus include every represented legal or visible-illegal candidate; only padding is excluded, and illegal policy logits are masked after scoring.</p></details>`}
const renderPreDomainState=renderState;renderState=run=>run.manifest.model_version>=61?renderDirectState(run):run.manifest.model_version>=60?renderZoneState(run):run.manifest.model_version>=53?renderDomainState(run):renderPreDomainState(run);
function flowNode([title,summary,details,color]){return `<div class=flow-node style="--cell:${color}" data-tip="${escapeAttribute(`${title}\n${details}`)}" tabindex=0><b>${title}</b><small>${summary}</small></div>`}function flowRow(nodes){return `<div class=flow-row>${nodes.map(flowNode).join('<div class=flow-arrow>→</div>')}</div>`}
function renderTraining(run){const target=document.querySelector('#trainingDiagram'),manifest=run.manifest,report=run.reports.at(-1),training=manifest.sessions?.at(-1)?.training||manifest.training||{},metrics=report.metrics||{},architecture=manifest.architecture||{},stage=report.stage||{},stages=manifest.stages||[],n=(value,digits=0)=>Number.isFinite(Number(value))?Number(value).toLocaleString(undefined,{maximumFractionDigits:digits}):'—',percent=value=>Number.isFinite(Number(value))?`${(100*Number(value)).toFixed(1)}%`:'—',envs=training.envs||0,samplerSteps=training.sampler_steps??training.rollout_steps??0,slice=envs&&samplerSteps?envs*samplerSteps:0,batch=training.batch||0,replay=architecture.winning_reservoir||0,progress=architecture.value_heads?.includes('progress'),actionLayers=architecture.action_layers||0,current=`A${stage.ascension??'?'} / +${stage.bonus??'?'}`,stageIndex=stage.index??0,next=stages[stageIndex+1],promotion=run.promotions.at(-1),diagrams=[];if(!('sampler_steps' in training)){const steps=report.pipeline||[];target.innerHTML=`<p><strong>Archived training design:</strong> this run predates the continuous asynchronous pipeline.</p><div class=training-diagrams><div class=training-diagram><h3>Reported pipeline</h3>${flowRow(steps.map((step,index)=>[`Step ${index+1}`,step,`${step}\nConfiguration is preserved in this run's manifest.`,['#38bdf8','#818cf8','#a78bfa','#e879f9','#22c55e'][index%5]]))}</div></div>`;return}diagrams.push(`<div class=training-diagram><h3>1. Asynchronous experience collection</h3>${flowRow([['Curriculum',`${current} · stage ${stageIndex+1}/${stages.length||'?'}`,`The collector creates environments at ascension ${stage.ascension??'?'} with training bonus ${stage.bonus??'?'}. Five characters share the same learner. Next stage: ${next?`A${next.ascension}/+${next.bonus}`:'none'}.`,'#38bdf8'],['CPU actor',`${n(envs)} parallel environments`,`One CPU collector thread runs ${n(envs)} simulator environments in fp32. Actions are sampled from the legal-action-masked categorical policy at temperature ${training.policy_temperature??1}. Training seed: ${training.training_seed??'?'}.`,'#60a5fa'],['Sampler slice',slice?`≤${n(slice)} decisions/call`:`${n(samplerSteps)} steps/environment`,`At most ${n(samplerSteps)} synchronous simulator steps are taken per environment, so a call produces at most ${n(slice)} decisions. Collection stops earlier when a complete episode becomes available. This is not a fixed-length rollout.`,'#818cf8'],['Complete trajectories',`${n(metrics.completed_trajectories)} accepted`,`Per-environment partial trajectories stay with the actor until terminal. Max ${n(training.max_steps)} steps/run and ${n(training.max_combat_steps)} steps/combat; truncated or empty-action trajectories are discarded. Latest discarded fraction: ${percent(metrics.discarded_step_fraction)}.`,'#a78bfa'],['Bounded queues',`weights 1 · samples 2`,`The learner publishes only when the one-slot model queue is empty; the actor drains to the newest version. Complete-trajectory packets cross a two-slot sample queue. Actor and accelerator learner run asynchronously.`,'#c084fc'],['Experience dataset',`peak ${n(metrics.dataset_peak)} rows`,`Latest accepted ${n(metrics.accepted_decisions)} of ${n(metrics.sampled_decisions)} sampled decisions. The transient dataset reached ${n(metrics.dataset_peak)} rows and is drained exactly once.`,'#e879f9']])}</div>`);diagrams.push(`<div class=training-diagram><h3>2. Returns, priorities, and minibatch selection</h3>${flowRow([['Three reward streams','win · shaped · progress',`Win reward is the terminal simulator reward. Policy reward adds the potential difference Vpotential(next) − Vpotential(current). Progress reward is the increase in maximum floor reached, divided by 52.`,'#22c55e'],['GAE and targets',`λ=${training.gae_lambda??'?'} · γprogress=${training.progress_gamma??'?'}`,`Policy advantage uses shaped reward with γ=1. Win-value return uses raw terminal reward with γ=1 and is clipped to [0,1]. Progress advantage and target use γ=${training.progress_gamma??'?'}; all use GAE λ=${training.gae_lambda??'?'}.`,'#14b8a6'],['Priority',`1+|Awin|+|Aprogress|+terminal/win bonuses`,`Each row gets priority 1 + |policy advantage| + |progress advantage| + 4×terminal + 4×winning-return. Sampling uses the square root of priority.`,'#06b6d4'],['Token-cost bucket',`one bucket per batch`,`Rows are bucketed by floor(log2(estimated token/attention cost)). A bucket is chosen by total priority to reduce padding and attention waste.`,'#0ea5e9'],['Balanced one-pass batch',`up to ${n(batch)} rows`,`Each batch draws evenly across the five characters when possible, without replacement, then deletes those rows from the dataset. Latest attempted/trained: ${n(metrics.dataset_attempted)} / ${n(metrics.dataset_trained)}.`,'#38bdf8'],['Freshness filters',`lag ≤${n(training.max_policy_lag)} · |log ratio| ≤${training.max_log_ratio??'?'}`,`Rows older than ${n(training.max_policy_lag)} learner updates are pruned. After forward evaluation, rows with |new logπ(a) − old logπ(a)| > ${training.max_log_ratio??'?'} are masked. Latest stale/ratio drops: ${n(metrics.dataset_stale_dropped)} / ${n(metrics.dataset_ratio_dropped)}.`,'#818cf8']])}</div>`);diagrams.push(`<div class=training-diagram><h3>3. PPO learner update</h3>${flowRow([['Model forward',`${architecture.layers??'?'} card layer${actionLayers?` + ${actionLayers} action-set layer`:''}`,`The selected model encodes the state and every represented action, then emits one masked policy logit per action, a win logit, and ${progress?'a progress value':'no progress head'}. Precision: ${training.precision??manifest.precision??'?'}.`,'#a78bfa'],['Normalize advantages',`per character · β=${n(metrics.progress_beta,3)}`,`Fresh win and progress advantages are independently z-normalized inside each character. Policy advantage = z(Awin) + β·z(Aprogress). β decays from 1 to 0.1 over ${n(training.progress_decisions)} decisions while the progress curriculum remains active.`,'#c084fc'],['Combined loss',`PPO ε=${training.clip??'?'} · value weight ${training.value_weight??'?'}`,`Loss = clipped PPO policy loss + ${training.value_weight??'?'}×win BCE${progress?` + ${training.value_weight??'?'}×progress MSE`:''} − entropy_weight×entropy. Entropy decays ${training.entropy_start??'?'} → ${training.entropy_end??'?'} over ${n(training.progress_decisions)} decisions.`,'#e879f9'],['Batch KL gate',`KL ≤ ${training.target_kl??'?'}`,`Mean sampled-action KL estimate is ratio − 1 − log_ratio over fresh rows. If it exceeds ${training.target_kl??'?'}, the entire optimizer step is skipped. Latest KL: ${n(metrics.kl,6)}; KL-dropped rows: ${n(metrics.dataset_kl_dropped)}.`,'#f472b6'],['Backpropagation',`global gradient norm ≤ 0.5`,`Gradients from fresh PPO rows and accepted winning replay are accumulated together, then clipped to global norm 0.5.`,'#f97316'],['Adam + publish',`lr ${training.learning_rate??'?'}`,`Adam uses learning rate ${training.learning_rate??'?'} and eps 1e-5. After a successful step, policy version increments and weights are offered to the actor. Latest learner/sampler versions: ${n(metrics.policy_version)} / ${n(metrics.sampler_version)}; mean lag ${n(metrics.policy_lag_mean,2)}.`,'#f59e0b']])}</div>`);if(replay)diagrams.push(`<div class=training-diagram><h3>4. Terminal winning replay</h3>${flowRow([['Pending episode sample',`per-env reservoir sampling`,`During each live episode, candidate decisions are reservoir-sampled into a bounded per-environment pending set. Stored data includes the chosen action, behavior log-probability, win value, and full behavior policy.`,'#84cc16'],['Admit winning rows',`${n(metrics.winning_reservoir)}/${n(replay)} stored`,`Only completed wins are admitted, and rows with advantage 1−Vwin ≤0 are skipped. Global capacity is ${n(replay)} with reservoir replacement. Latest wins/seen/skipped: ${n(metrics.winning_episodes)} / ${n(metrics.winning_seen)} / ${n(metrics.winning_skipped)}.`,'#22c55e'],['Balanced compatible sample','≤10% of update data',`Candidates are round-robin balanced across characters. At most fresh_batch/9 rows survive, making replay at most 10% of fresh+replay data. Only candidates no more expensive than the fresh token-cost bucket are packed.`,'#14b8a6'],['Categorical KL check',`≤${training.target_kl??'?'}`,`The stored full behavior distribution is compared with the current masked policy. Eligible rows above KL ${training.target_kl??'?'} are rejected and their reference policy is re-anchored to the current model. Latest accepted/rejected: ${n(metrics.winning_replayed)} / ${n(metrics.winning_rejected_kl)}.`,'#06b6d4'],['Replay PPO term',`weight ${metrics.winning_loss_weight??.1}`,`Replay advantage is stop_gradient(1 − sigmoid(win logit)). The selected-action ratio uses the stored reference policy and the same PPO clip ε=${training.clip??'?'}. Its loss is multiplied by ${metrics.winning_loss_weight??.1}.`,'#0ea5e9']])}</div>`);diagrams.push(`<div class=training-diagram><h3>${replay?'5':'4'}. Reporting, checkpoints, and promotion</h3>${flowRow([['Live report',`every ${n(training.report_decisions)} decisions`,`At each handled-decision boundary, metrics are aggregated, live.json and an immutable report are written, latest.pt is refreshed, and this dashboard is regenerated. Latest step: ${n(report.step)}.`,'#38bdf8'],['Best checkpoint',`score improves lexicographically`,`A report becomes best when it improves: characters with any win; then worst per-character floor penalized by caps; then mean floor; then aggregate win rate.`,'#60a5fa'],['Periodic checkpoint',`every ${n(training.save_decisions)} decisions`,`An immutable numbered checkpoint plus latest.pt and SHA-256 metadata are written every ${n(training.save_decisions)} handled decisions.`,'#818cf8'],['Promotion trigger',`${n(training.promotion_window)} episodes/character`,`The last ${n(training.promotion_window)} training episodes for every character must each reach at least ${percent(training.promote_win_rate)} wins. This stops the actor and starts deterministic evaluation.`,'#a78bfa'],['Greedy evaluation',`${n(training.promotion_runs)} runs × 5 characters`,`Argmax actions run in batches of ${n(training.evaluation_batch)}, with ${n(training.evaluation_max_steps)}-step run and ${n(training.evaluation_max_combat_steps)}-step combat caps. Promotion seed advances independently.`,'#c084fc'],['Stage outcome',promotion?`${promotion.promoted?'promoted':'stayed'} at ${n(promotion.step)}`:'no check yet',`Every character must win at least ${percent(training.promote_win_rate)} of all evaluation runs. Pass: advance to ${next?`A${next.ascension}/+${next.bonus}`:'the final stage'} and clear winning replay. Once every character reaches 10%, the auxiliary progress advantage is permanently disabled. Training stops at ${training.hours?`${training.hours} hour${training.hours===1?'':'s'}`:'no time cap'} or ${training.decisions?n(training.decisions)+' decisions':'no decision cap'}, whichever comes first.`,'#e879f9']])}</div>`);target.innerHTML=`<p><strong>Latest report:</strong> ${n(report.step)} decisions · ${n(metrics.updates)} optimizer updates · ${n(metrics.decisions_per_second,1)} decisions/s</p><div class=training-diagrams>${diagrams.join('')}</div>`}
const stateTip=document.querySelector('#stateTip');function showStateTip(cell,x,y){stateTip.textContent=cell.dataset.tip;stateTip.style.display='block';stateTip.style.left=Math.min(x+14,innerWidth-stateTip.offsetWidth-12)+'px';stateTip.style.top=Math.min(y+14,innerHeight-stateTip.offsetHeight-12)+'px'}document.addEventListener('pointerover',event=>{const cell=event.target.closest('[data-tip]');if(cell)showStateTip(cell,event.clientX,event.clientY)});document.addEventListener('pointermove',event=>{const cell=event.target.closest('[data-tip]');if(cell)showStateTip(cell,event.clientX,event.clientY)});document.addEventListener('pointerout',event=>{if(event.target.closest('[data-tip]'))stateTip.style.display='none'});document.addEventListener('focusin',event=>{const cell=event.target.closest('[data-tip]');if(cell){const rect=cell.getBoundingClientRect();showStateTip(cell,rect.right,rect.bottom)}});document.addEventListener('focusout',event=>{if(event.target.closest('[data-tip]'))stateTip.style.display='none'});
function details(run){const manifest=run.manifest,report=run.reports.at(-1),training=manifest.sessions?.at(-1)?.training||manifest.training||{},heads=manifest.architecture?.value_heads||[],progress=heads.includes('progress'),replay=manifest.architecture?.winning_reservoir,weight=training.value_weight??'?',replayWeight=report.metrics.winning_loss_weight??.1,architecture=Object.entries(manifest.architecture||{}).map(([key,value])=>`${key}=${Array.isArray(value)?value.join('/'):value}`).join(' · '),metrics=Object.entries(report.metrics).filter(([,value])=>typeof value==='number').map(([key,value])=>`<tr><td>${key}</td><td>${Number(value).toLocaleString(undefined,{maximumSignificantDigits:6})}</td></tr>`).join(''),configuration=['precision','learning_rate','policy_temperature','batch','envs','sampler_steps','rollout_steps','clip','gae_lambda','progress_gamma','progress_decisions','target_kl'].filter(key=>key in training).map(key=>`${key}=${training[key]}`).join(' · '),entropy=`${training.entropy_start??'?'}→${training.entropy_end??'?'}${training.progress_decisions?` over ${Number(training.progress_decisions).toLocaleString()} decisions`:''}`,advantage=progress?`per-character z(Awin) + β·z(Aprogress); GAE γwin=1, γprogress=${training.progress_gamma}, λ=${training.gae_lambda}; β=1→0.1 while active`:`win GAE γ=1, λ=${training.gae_lambda}`,winLoss=heads.includes('win_logit')?'terminal win/death BCE':'terminal win/death value regression',loss=`PPO-clip ε=${training.clip} + ${weight}×${winLoss}${progress?` + ${weight}×progress MSE`:''} − entropy×H${replay?` + ${replayWeight}×winning-replay PPO`:''}`,replayRule=replay?`; replay advantage=1−Vwin, data≤10%, categorical KL≤${training.target_kl}`:'';return `<p><strong>${manifest.change||report.description}</strong></p><p>${report.description}</p><p><strong>Architecture</strong> ${architecture}</p><p><strong>Training</strong> ${configuration}</p><p><strong>Entropy schedule</strong> ${entropy}</p><p><strong>Advantage</strong> ${advantage}</p><p><strong>Loss</strong> ${loss}${replayRule}</p><div class=pipeline>${report.pipeline.map(step=>`<span>${step}</span>`).join('')}</div><details><summary>Latest measured metrics</summary><table><tbody>${metrics}</tbody></table></details>`}
function promotionSummary(row){if(!row)return '<p>No promotion check yet.</p>';const characters=row.result?.characters||[],rows=characters.map(item=>`<tr><td>${characterNames[item.character]??`Character ${item.character}`}</td><td>${item.wins}/${item.runs}</td><td>${(100*item.wins/item.runs).toFixed(1)}%</td><td>${Number(item.floor_mean).toFixed(2)}</td><td>${item.caps}</td></tr>`).join('');return `<p class="${row.promoted?'yes':'no'}">${row.promoted?'Promoted':'Stayed at current stage'} · threshold ${(100*row.threshold).toFixed(0)}% per character · seed ${row.seed}</p><table><thead><tr><th>Character</th><th>Wins</th><th>Rate</th><th>Mean floor</th><th>Caps</th></tr></thead><tbody>${rows}</tbody></table>`}
function saveDashboardState(){const views={};document.querySelectorAll('.plot').forEach(node=>{const view={};if(node._fullLayout?.xaxis?.autorange===false)view.x=[...node._fullLayout.xaxis.range];if(node._fullLayout?.yaxis?.autorange===false)view.y=[...node._fullLayout.yaxis.range];if(view.x||view.y)views[node.id]=view});try{sessionStorage.setItem(refreshKey,JSON.stringify({version:versionSelect.value,followLatest:versionSelect.value===names.at(-1),xaxis:xaxis.value,smooth:smooth.checked,ema:ema.value,scroll:[scrollX,scrollY],views}))}catch{}}
function restoreDashboardState(){if(saved?.version===versionSelect.value&&saved.xaxis===xaxis.value)for(const [id,view] of Object.entries(saved.views||{})){const update={};if(view.x)update['xaxis.range']=view.x;if(view.y)update['yaxis.range']=view.y;if(Object.keys(update).length)Plotly.relayout(id,update)}if(saved?.scroll)scrollTo(...saved.scroll)}
function showVersion(){const run=versions[versionSelect.value],reports=run.reports,timed=reports.find(row=>Number.isFinite(Number(row._written))&&Number.isFinite(Number(row.metrics.seconds)));timeOrigin=timed?timed._written-timed.metrics.seconds:0;document.querySelector('#title').textContent=`Spirefysh V${run.version} · ${run.run}`;document.querySelector('#details').innerHTML=details(run);renderState(run);renderTraining(run);plot('advantage',series(reports,'mean_advantage'),{history:reports,run});plot('throughput',series(reports,'decisions_per_second'),{tozero:true,history:reports,run});floorPlot(reports,run);stagePlot('ascension',reports,'ascension','#fb7185',run);stagePlot('bonus',reports,'bonus','#f59e0b',run);plot('wins',reports.map(row=>({x:x(row),y:row.metrics.wins/Math.max(1,row.metrics.episodes)})),{range:[0,1],percent:true,history:reports,run});plot('clip',series(reports,'clip_fraction'),{range:[0,1],percent:true,history:reports,run});plot('kl',series(reports,'kl'),{tozero:true,history:reports,run});plot('entropy',series(reports,'entropy'),{tozero:true,history:reports,run});document.querySelector('#promotion').innerHTML=promotionSummary(run.promotions.at(-1))}versionSelect.onchange=showVersion;xaxis.onchange=showVersion;smooth.onchange=showVersion;ema.oninput=showVersion;showVersion();setTimeout(restoreDashboardState,100);window.addEventListener('beforeunload',saveDashboardState);setInterval(()=>{saveDashboardState();location.reload()},15000)</script>"""
    floor_start = content.index("function floorPlot(")
    floor_end = content.index("function stagePlot(", floor_start)
    content = content[:floor_start] + r"""function floorPlot(history,run){const completed=history.filter(report=>Number(report.metrics.episodes)>0||(report.metrics.trajectory_floors?.length??0)>0),points=[],traces=[],bands=[['min','max','min–max','.04'],['p01','p99','p1–p99','.06'],['p05','p95','p5–p95','.09'],['p10','p90','p10–p90','.13'],['p25','p75','p25–p75','.20']];for(const [low,high,name,alpha] of bands){const rows=completed.map(report=>({x:x(report),low:report.metrics.floor_bands?.[low],high:report.metrics.floor_bands?.[high]})).filter(row=>Number.isFinite(row.low)&&Number.isFinite(row.high));traces.push({x:rows.map(row=>row.x),y:rows.map(row=>row.low),mode:'lines',showlegend:false,hoverinfo:'skip',line:{width:0}},{x:rows.map(row=>row.x),y:rows.map(row=>row.high),mode:'lines',name,line:{width:0},fill:'tonexty',fillcolor:`rgba(111,177,255,${alpha})`,hovertemplate:`${name}<br>upper %{y:.2f}<extra></extra>`})}for(const report of completed){let inferred=0,last=-Infinity;for(const row of report.metrics.trajectory_floors||[]){const [iteration,floor,stored]=row;if(stored===undefined&&iteration<last)inferred++;const character=stored??Math.min(inferred,4);points.push({x:xaxis.value==='iteration'?iteration:xaxis.value==='decisions'?report.step:(report.metrics.seconds||0)/60,y:floor,character,iteration,step:report.step});last=iteration}}for(let i=points.length-1;i>0;i--){const j=Math.floor(Math.random()*(i+1));[points[i],points[j]]=[points[j],points[i]]}traces.push({x:points.map(point=>point.x),y:points.map(point=>point.y),customdata:points.map(point=>[characterNames[point.character],point.iteration,point.step]),mode:'markers',name:'trajectories',marker:{color:points.map(point=>characterColors[point.character]),size:5,opacity:.28},hovertemplate:'%{customdata[0]}<br>floor %{y}<br>iteration %{customdata[1]:,}<br>report decisions %{customdata[2]:,}<extra></extra>'});const means=completed.map(report=>({x:x(report),y:report.metrics.floor_bands?.mean??report.metrics.floor_mean})).filter(point=>Number.isFinite(point.y));traces.push({x:means.map(point=>point.x),y:means.map(point=>point.y),mode:'lines+markers',name:'mean',line:{color:'#ffb454',width:3},marker:{size:5},hovertemplate:'mean %{y:.2f}<extra></extra>'});if(smooth.checked&&means.length>1){const line=emaLine(means);traces.push({x:line.map(point=>point.x),y:line.map(point=>point.y),mode:'lines',name:`mean EMA α=${Number(ema.value)||.2}`,line:{color:'#f97316',width:2,dash:'dot'},hovertemplate:'mean EMA %{y:.2f}<extra></extra>'})}for(const [character,name] of characterNames.entries())traces.push({x:[null],y:[null],mode:'markers',name,hoverinfo:'skip',marker:{color:characterColors[character],size:7,opacity:.6}});const options=layout(false,[0,52],history,run);options.yaxis.title='Terminal floor';Plotly.react('floor',traces,options,config)}
""" + content[floor_end:]
    content = content.replace(
        ".state-flow{display:grid;grid-template-columns:1fr auto 1fr auto 1fr auto 1.2fr;gap:10px;align-items:stretch;margin:14px 0}.state-flow.action-flow{grid-template-columns:1fr auto 1fr auto 1fr}",
        ".state-flow{display:flex;gap:10px;align-items:stretch;margin:14px 0;overflow-x:auto}.state-flow>.state-node{flex:1;min-width:150px}",
    ).replace(
        "['Token-cost bucket',`one bucket per batch`,`Rows are bucketed by floor(log2(estimated token/attention cost)). A bucket is chosen by total priority to reduce padding and attention waste.",
        "['Full minibatch',`up to ${n(batch)} rows`,`Rows are sampled across action counts because the direct policy head has no quadratic action-set attention.",
    ).replace(
        "`${architecture.layers??'?'} card layer${actionLayers?` + ${actionLayers} action-set layer`:''}`",
        "architecture.candidate_attention?`card/map/fusion L${architecture.layers??'?'} + one-query candidates`:`${architecture.layers??'?'} state layer${actionLayers?` + ${actionLayers} decision layer`:''}`",
    ).replace(
        "Only candidates no more expensive than the fresh token-cost bucket are packed.",
        "Replay candidates are sampled independently and remain capped at 10% of update rows.",
    ).replace(
        "At each handled-decision boundary, metrics are aggregated, live.json and an immutable report are written, latest.pt is refreshed, and this dashboard is regenerated.",
        "At each handled-decision boundary, metrics are aggregated, live.json and an immutable report are written, and this dashboard is regenerated.",
    ).replace(
        "A report becomes best when it improves:",
        "A periodic checkpoint becomes best when its report improves:",
    ).replace(
        "['Best checkpoint',`score improves lexicographically`,`A periodic checkpoint becomes best when its report improves:",
        "['Per-stage best',`one immutable checkpoint per stage`,`Each stage retains its best periodic checkpoint by:",
    ).replace(
        "handled decisions.`,'#818cf8']",
        "handled decisions. Every stage-specific best survives pruning; the current periodic checkpoint is also retained during its training session.`,'#818cf8']",
    ).replace(
        "['CPU actor',`${n(envs)} parallel environments`,`One CPU collector thread runs ${n(envs)} simulator environments in fp32.",
        "['CPU actors',`${n(training.samplers??1)} processes × ${n(Math.floor(envs/(training.samplers??1)))} environments`,`${n(training.samplers??1)} CPU sampler processes run ${n(envs)} total simulator environments in fp32 using ${n(training.sampler_threads??1)} Rayon simulator threads and ${n(training.torch_threads??1)} Torch inference thread${(training.torch_threads??1)===1?'':'s'} each.",
    ).replace(
        "['Complete trajectories',`${n(metrics.completed_trajectories)} accepted`,`Per-environment partial trajectories stay with the actor until terminal.",
        "['Trajectory units',`${n(metrics.completed_trajectories)} accepted`,`${training.segment_steps?`Training transport uses ${n(training.segment_steps)}-decision segments bootstrapped only at live boundaries; episode statistics and winning replay still wait for true terminals.`:'Per-environment partial trajectories stay with the actor until terminal.'}",
    ).replace(
        "['Bounded queues',`weights 1 · samples 2`,`The learner publishes only when the one-slot model queue is empty; the actor drains to the newest version. Complete-trajectory packets cross a two-slot sample queue. Actor and accelerator learner run asynchronously.",
        "['Bounded queues',`weights ${n(training.samplers??1)} · samples 2`,`Each sampler has a one-slot model queue and drains to the newest version. Trajectory units share a two-slot sample queue. A heartbeat watchdog restarts stalled samplers after ${n(training.sampler_timeout??120)} seconds.",
    ).replace(
        "['Three reward streams','win · shaped · progress',`Win reward is the terminal simulator reward. Policy reward adds the potential difference Vpotential(next) − Vpotential(current). Progress reward is the increase in maximum floor reached, divided by 52.",
        "['Two reward streams','terminal win · progress',`Policy and win-value targets use the raw terminal simulator reward. The auxiliary progress target is the increase in maximum floor reached, divided by 52.",
    ).replace(
        "Policy advantage uses shaped reward with γ=1. Win-value return uses raw terminal reward with γ=1",
        "Policy advantage and win-value return use raw terminal reward with γ=1",
    ).replace(
        "and their reference policy is re-anchored to the current model.",
        "and are evicted; behavior references are immutable.",
    ).replace(
        "After a successful step, policy version increments and weights are offered to the actor.",
        "After a successful step, policy version increments; weights are published every ${n(training.publish_updates??1)} accepted updates.",
    ).replace(
        "'sampler_steps','rollout_steps'",
        "'sampler_steps','segment_steps','rollout_steps'",
    ).replace(
        "'progress_gamma','progress_decisions','target_kl'",
        "'progress_gamma','progress_decisions','progress_beta','target_kl'",
    ).replace(
        "β=1→0.1 while active",
        "${training.progress_beta==null?`run-global β=${report.metrics.progress_beta??'—'} (1→0.1)`:`β=${training.progress_beta} fixed by --progress-beta`}",
    ).replace(
        "β decays from 1 to 0.1 over ${n(training.progress_decisions)} decisions while the progress curriculum remains active.",
        "${training.progress_beta==null?`β decays once from 1 to 0.1 over ${n(training.progress_decisions)} run-global decisions, then stays at 0.1 through A0/+0.`:`β is fixed at ${n(training.progress_beta,3)} by --progress-beta.`}",
    ).replace(
        "Entropy decays ${training.entropy_start??'?'} → ${training.entropy_end??'?'} over ${n(training.progress_decisions)} decisions.",
        "Entropy follows the stage-local ${n(metrics.curriculum?.entropy_start,4)} → ${n(metrics.curriculum?.entropy_end,4)} schedule over ${n(training.progress_decisions)} decisions.",
    ).replace(
        "Once every character reaches 10%, the auxiliary progress advantage is permanently disabled.",
        "${training.progress_beta==null?'The auxiliary remains active through A0/+0 and is disabled only on promotion into A1/+0.':`The explicit fixed β=${n(training.progress_beta,3)} override also disables only on promotion into A1/+0.`}",
    ).replace(
        "Each stage retains its best periodic checkpoint by: characters with any win; then worst per-character floor penalized by caps; then mean floor; then aggregate win rate.",
        "Each stage retains the checkpoint with the best score on the fixed ${n(training.development_runs)}-run-per-character development panel: character coverage, worst Wilson lower bound, aggregate win rate, cap-penalized worst floor, then mean floor.",
    ).replace(
        "An immutable numbered checkpoint plus latest.pt and SHA-256 metadata are written every ${n(training.save_decisions)} handled decisions.",
        "An immutable numbered checkpoint plus latest.pt and SHA-256 metadata are written every ${n(training.save_decisions)} handled decisions, then evaluated on fixed development seed ${training.development_seed??'?'}.",
    ).replace(
        "each reach at least ${percent(training.promote_win_rate)} wins. This stops the actor and starts deterministic evaluation.",
        "each reach at least ${percent(training.promotion_trigger_rate??training.promote_win_rate)} wins. This schedules the independent promotion panel; it is not the promotion decision.",
    ).replace(
        "xaxis.value==='decisions'?iteration*envs:",
        "xaxis.value==='decisions'?(training.samplers?report.step:iteration*envs):",
    ).replace(
        "${report.pipeline.map(step=>`<span>${step}</span>`).join('')}",
        "${report.pipeline.map(step=>`<span>${manifest.model_version>=51&&step.includes('shared card-zone Transformer')?`${architecture.layers}-layer card/actor/map fusion → candidate-conditioned policy and critic`:step}</span>`).join('')}",
    )
    target.mkdir(parents=True, exist_ok=True)
    temporary = target / "dashboard.html.tmp"
    temporary.write_text(content)
    temporary.replace(target / "dashboard.html")


def train(args):
    torch.set_num_threads(args.torch_threads)
    torch.manual_seed(args.seed); np.random.seed(args.seed)
    target = device()
    if args.precision not in PRECISIONS:
        raise ValueError("precision must be fp32 or bf16")
    if args.precision != "fp32" and target.type not in ("mps", "cuda"):
        raise ValueError(f"{args.precision} requires MPS or CUDA")
    if min(args.envs, args.samplers, args.torch_threads, args.sampler_threads,
           args.batch, args.sampler_steps,
           args.publish_updates,
           args.report_decisions,
           args.save_decisions, args.promotion_window, args.promotion_runs,
           args.development_runs, args.progress_decisions,
           args.max_policy_lag + 1) < 1 or min(args.max_log_ratio, args.policy_temperature) <= 0:
        raise ValueError("invalid asynchronous replay settings")
    if args.sampler_timeout <= 0 or args.sampler_restarts < 0 or not 0 < args.progress_gamma <= 1:
        raise ValueError("invalid sampler watchdog or progress discount")
    if args.segment_steps < 0:
        raise ValueError("invalid segment length")
    if args.envs % args.samplers:
        raise ValueError("environments must be divisible by samplers")
    if args.hours <= 0 and args.decisions <= 0:
        raise ValueError("set --hours or --decisions")
    if not 0 <= args.promotion_trigger_rate <= args.promote_win_rate <= 1:
        raise ValueError("invalid promotion rates")
    if seed_panel(args.development_seed, args.development_runs)[1] > args.promotion_seed:
        raise ValueError("development and promotion seed panels overlap")
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
            (args.width, args.layers, args.heads, args.feedforward), (64, 2, 4, 128)
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
    output.mkdir(parents=True, exist_ok=continuing)
    training_lock = (output / ".train.lock").open("a+")
    try:
        fcntl.flock(training_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as error:
        raise RuntimeError(f"trainer already running for {output}") from error
    training_lock.seek(0); training_lock.truncate(); training_lock.write(str(os.getpid())); training_lock.flush()
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
    if continuing and manifest.get("model_version") != MODEL_VERSION:
        raise ValueError("new model versions require a new output directory")
    manifest["model_version"] = MODEL_VERSION
    manifest["change"] = CHANGE
    manifest["precision"] = args.precision
    manifest["parameters"] = sum(parameter.numel() for parameter in model.parameters())
    manifest["architecture"] = architecture(model)
    evaluation_panels = {
        "development": {"seed": args.development_seed, "runs_per_character": args.development_runs},
        "promotion": {
            "seed": args.promotion_seed, "stride": 100_000_000,
            "runs_per_character": args.promotion_runs,
        },
    }
    if continuing and manifest.get("evaluation_panels") not in (None, evaluation_panels):
        raise ValueError("evaluation seed panels cannot change within a run")
    manifest["evaluation_panels"] = evaluation_panels
    manifest["curriculum"] = {
        "stage_local_decay_decisions": args.progress_decisions,
        "progress_beta": (
            f"fixed {args.progress_beta}; off after A0/+0 promotion"
            if args.progress_beta is not None
            else "run-global 1 -> 0.1; held through A0/+0; off on A1/+0 entry"
        ),
        "entropy": "(start -> end) * max(0.35, 0.8^stage)",
    }
    manifest.setdefault("sessions", []).append({"step": source["decisions"] if source else 0, "training": training})
    atomic_json(output / "run.json", manifest)
    reports_dir = output / "reports"; reports_dir.mkdir(exist_ok=continuing)
    checkpoints_dir = output / "checkpoints"; checkpoints_dir.mkdir(exist_ok=continuing)
    development_dir = output / "development"; development_dir.mkdir(exist_ok=continuing)
    champions_dir = output / "stage-champions"; champions_dir.mkdir(exist_ok=continuing)
    entries_dir = output / "stage-entries"; entries_dir.mkdir(exist_ok=continuing)
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
                logits = predict(
                    model, inputs, args.precision, args.policy_temperature,
                )[0].masked_fill(~inputs[6], -torch.inf)
                policies = logits.log_softmax(-1).cpu().numpy()
            for index, policy in zip(indices, policies):
                row = reservoir.rows[index]
                reservoir.rows[index] = (*row[:4], np.asarray(policy[:len(row[0][4])], np.float16))
    decisions = source["decisions"] if source else 0
    auxiliary_decisions = source.get("auxiliary_decisions", source["decisions"]) if source else 0
    stage_decisions = resume_stage_decisions(source, stage)
    progress_active = stage <= 6
    sampler_session = source.get("sampler_session", source.get("sampler_index", 0)) if source else 0
    promotion_index = source.get("promotion_index", 0) if source else 0
    if not 0 <= stage < len(STAGES):
        raise ValueError("invalid checkpoint stage")
    if args.decisions and args.decisions < decisions:
        raise ValueError("--decisions precedes the checkpoint")
    if args.decisions and (args.decisions - decisions) % args.envs:
        raise ValueError("remaining decisions must be divisible by environments")
    if args.promote_now and (not source or args.decisions != decisions):
        raise ValueError("--promote-now requires an unchanged checkpoint decision target")
    initial = None
    initial_digest = None
    if not source and not continuing:
        initial = output / "initial.pt"
        initial_digest = save_checkpoint(
            initial, model, optimizer, manifest, stage, 0, sampler_session,
            promotion_index, progress_active, reservoir, auxiliary_decisions, stage_decisions,
        )
        atomic_json(output / "initial.json", {
            "step": 0, "checkpoint": initial.name, "sha256": initial_digest,
        })
    latest = output / "latest.pt"
    def save(path, step, replace):
        return save_checkpoint(
            path, model, optimizer, manifest, stage, step, sampler_session,
            promotion_index, progress_active, reservoir, auxiliary_decisions + step - decisions,
            stage_decisions + step - decisions, replace,
        )
    if initial is not None and decisions == 0:
        link_checkpoint(initial, latest)
        digest = initial_digest
    else:
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
    best_path = output / "best.json"
    best = json.loads(best_path.read_text()) if best_path.exists() else {}
    best_score = tuple(best.get("global_score", ()))
    stage_bests = load_stage_bests(output, development_dir, checkpoints_dir, best)
    for key, record in stage_bests.items():
        champion = champions_dir / f"{int(key):02}.pt"
        if not champion.exists():
            link_checkpoint(output / record["checkpoint"], champion)
        record["artifact"] = str(champion.relative_to(output))
    if stage_bests != best.get("stages", {}):
        best["stages"] = stage_bests
        atomic_json(best_path, best)
    def promote():
        nonlocal stage, stage_decisions, promotion_index, progress_active, promotion_seconds
        path = promotions_dir / f"{decisions:012}.json"
        if path.exists() or stage + 1 >= len(STAGES):
            raise ValueError("promotion is not available at this checkpoint")
        promotion_started = time.monotonic()
        seed = args.promotion_seed + promotion_index * 100_000_000 + stage * 10_000_000
        result = evaluate(
            model, args, target, seed, args.promotion_runs, stage,
            args.evaluation_max_steps, args.evaluation_max_combat_steps,
        )
        promoted = all(row["wins"] / row["runs"] >= args.promote_win_rate
                       for row in result["characters"])
        promotion_index += 1
        promotion = {
            "schema": 1, "step": decisions, "seed": seed,
            "seconds": elapsed_offset + time.monotonic() - started,
            "stage": {"index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1]},
            "trigger": {
                "episodes_per_character": args.promotion_window,
                "win_rate": args.promotion_trigger_rate,
            },
            "threshold": args.promote_win_rate, "promoted": promoted, "result": result,
        }
        immutable_json(path, promotion)
        if promoted:
            progress_active &= stage != 6
            stage += 1
            stage_decisions = 0
            reservoir.clear()
            entry = entries_dir / f"{stage:02}-{decisions:012}.pt"
            entry_digest = save(entry, decisions, False)
            best.setdefault("entries", {})[str(stage)] = {
                "step": decisions,
                "stage": {"index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1]},
                "checkpoint": str(entry.relative_to(output)), "sha256": entry_digest,
            }
            atomic_json(best_path, best)
        promotion_seconds += time.monotonic() - promotion_started
        digest = save(latest, decisions, True)
        atomic_json(output / "latest.json", {
            "step": decisions, "stage": stage, "sampler_session": sampler_session,
            "checkpoint": latest.name, "sha256": digest,
        })
        dashboard(output.parent)
        print(json.dumps(promotion), flush=True)

    if args.promote_now:
        promote()
    while time.monotonic() < deadline and (not args.decisions or decisions < args.decisions):
        sampler_session += 1
        base = decisions
        budget = args.decisions - decisions if args.decisions else (1 << 62) // args.envs * args.envs
        last_checkpoint = last_development = None
        def save_latest(step):
            digest = save(latest, step, True)
            atomic_json(output / "latest.json", {
                "step": step, "stage": stage, "sampler_session": sampler_session,
                "checkpoint": latest.name, "sha256": digest,
            })
        def save_step(step):
            nonlocal last_checkpoint, last_development, best_score
            checkpoint = checkpoints_dir / f"{step:012}.pt"
            immutable_digest = save(checkpoint, step, False)
            link_checkpoint(checkpoint, latest)
            digest = immutable_digest
            last_checkpoint = step, checkpoint, immutable_digest
            result = evaluate(
                model, args, target, args.development_seed, args.development_runs, stage,
                args.evaluation_max_steps, args.evaluation_max_combat_steps,
            )
            score = evaluation_score(result)
            evaluation = {
                "schema": 1, "step": step, "seed": args.development_seed,
                "stage": {"index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1]},
                "score": score, "result": result,
            }
            immutable_json(development_dir / f"{step:012}.json", evaluation)
            last_development = evaluation
            record = {
                "step": step, "score": score, "stage": evaluation["stage"],
                "checkpoint": str(checkpoint.relative_to(output)),
                "sha256": immutable_digest, "development": result,
            }
            if score > tuple(stage_bests.get(str(stage), {}).get("score", ())):
                champion = champions_dir / f"{stage:02}.pt"
                link_checkpoint(checkpoint, champion)
                record["artifact"] = str(champion.relative_to(output))
                stage_bests[str(stage)] = record
            global_score = (stage, *score)
            if global_score > best_score:
                best_score = global_score
                best.update(record | {"global_score": global_score})
            best["stages"] = stage_bests
            atomic_json(best_path, best)
            atomic_json(output / "latest.json", {
                "step": step, "stage": stage, "sampler_session": sampler_session,
                "checkpoint": latest.name,
                "immutable": str(checkpoint.relative_to(output)),
                "immutable_sha256": immutable_digest, "sha256": digest,
            })
        def save_report(point, pipeline, window):
            if last_development and last_development["step"] == point["steps"]:
                point["development"] = last_development["result"]
            row = {
                "schema": 1, "step": point["steps"], "window": window,
                "sampler_session": sampler_session,
                "stage": {"index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1]},
                "description": f"Continuous V{MODEL_VERSION} training at A{STAGES[stage][0]}/+{STAGES[stage][1]}.",
                "pipeline": pipeline, "metrics": point,
            }
            immutable_json(reports_dir / f"{point['steps']:012}.json", row)
            atomic_json(output / "live.json", row)
            if not last_checkpoint or last_checkpoint[0] != point["steps"]:
                save_latest(point["steps"])
            keep = {last_checkpoint[1]} if last_checkpoint else set()
            keep.update(output / row["checkpoint"] for row in stage_bests.values())
            if best.get("checkpoint"):
                keep.add(output / best["checkpoint"])
            for checkpoint in checkpoints_dir.glob("*.pt"):
                if checkpoint not in keep:
                    checkpoint.unlink()
            dashboard(output.parent)
            print(json.dumps(row), flush=True)
        model.train()
        training = train_stream(
            model, optimizer, args, sampler_session, stage, target, deadline, budget,
            base, auxiliary_decisions, stage_decisions, run_started, reservoir, progress_active,
            save_report, save_step,
        )
        if not training["decisions"]:
            break
        decisions += training["decisions"]
        auxiliary_decisions += training["decisions"]
        stage_decisions += training["decisions"]
        training_seconds += training["seconds"]
        if not training["promotion_ready"] or time.monotonic() >= deadline:
            break
        promote()
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
    version = checkpoint.get("model_version")
    if checkpoint.get("schema") != 1 or version != MODEL_VERSION:
        raise ValueError("incompatible checkpoint")
    if checkpoint.get("feature_version") != FEATURE_VERSION or layout["version"] != FEATURE_VERSION:
        raise ValueError("incompatible feature version")
    if checkpoint["fingerprint"] != live.fingerprint() or checkpoint["layout"] != layout:
        raise ValueError("checkpoint does not match simulator content")
    config = checkpoint.get("architecture", {})
    try:
        model = Agent(layout, *(config[key] for key in (
            "width", "layers", "heads", "feedforward", "head_width"
        ))).to(target)
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError("invalid checkpoint architecture") from error
    if config != architecture(model):
        raise ValueError("incompatible model architecture")
    missing, unexpected = model.load_state_dict(checkpoint["model"], strict=version == MODEL_VERSION)
    if missing or unexpected:
        raise ValueError("incompatible model migration")
    checkpoint["_optimizer_compatible"] = migrate_optimizer(
        checkpoint["optimizer"], len(tuple(model.parameters()))
    )
    checkpoint["model_version"] = MODEL_VERSION
    checkpoint["architecture"] = architecture(model)
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
                legal = observation_legal(observation)
                empty = active & ~legal.any(1)
                for index in np.flatnonzero(empty):
                    pending[index].clear()
                caps += int(empty.sum()); active &= ~empty
                if not active.any():
                    break
                indices = np.flatnonzero(active)
                packed = pack_batch(observation)
                if len(packed) != count:
                    raise ValueError("packed observation row count mismatch")
                inputs = unpack([packed[index] for index in indices], target, model)
                with torch.no_grad(), torch.autocast(
                    target.type, dtype=torch.bfloat16 if args.precision == "bf16" else torch.float16,
                    enabled=args.precision != "fp32",
                ):
                    logits, _, _ = model(*inputs[:6])
                masked = logits.float().masked_fill(~inputs[6], -torch.inf)
                selected = masked.argmax(1).cpu().numpy()
                choice = np.zeros(count, np.int64); choice[indices] = selected
                for index in np.flatnonzero(active):
                    seen[index] += 1
                    if len(pending[index]) < args.states_per_run:
                        pending[index].append(packed[index])
                    else:
                        slot = int(rng.integers(seen[index]))
                        if slot < args.states_per_run:
                            pending[index][slot] = packed[index]
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
    return states, np.asarray(labels, np.float32), np.asarray(characters), reports


def value_logits(model, states, batch, target):
    values = []
    model.eval()
    with torch.no_grad():
        for start in range(0, len(states), batch):
            inputs = unpack(states[start : start + batch], target, model)
            values.extend(model(*inputs[:6])[1].cpu().tolist())
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
        struct.pack("<18I", model.width, model.layers, model.heads, model.feedforward,
                    model.card_zones, len(DOMAIN_SPECS), model.layout["globals"], 8,
                    model.entity_collections, model.head_width,
                    ACTION_FIELDS[0], ACTION_FIELDS[1], ACTION_FIELDS[3],
                    model.semantic.num_embeddings, ACTION_FIELDS[2], ACTION_FIELDS[3],
                    6, model.state_width),
        struct.pack("<64I", *(value for _name, unsigned, signed, semantic, numeric in DOMAIN_SPECS
                              for value in (unsigned, signed, semantic, numeric))),
        struct.pack("<2f", temperature, bias),
    ]

    def add(value):
        parts.append(value.detach().float().cpu().contiguous().numpy().astype("<f4", copy=False).tobytes())

    def add_linear(module):
        add(module.weight)
        if module.bias is not None:
            add(module.bias)

    def add_norm(module):
        add(module.weight); add(module.bias)

    def add_encoder(module):
        add(module.field); add(module.numeric.weight); add(module.bias); add_norm(module.norm)

    def add_transformer(transformer):
        for layer in transformer.layers:
            for value in (
                layer.self_attn.in_proj_weight, layer.self_attn.in_proj_bias,
                layer.self_attn.out_proj.weight, layer.self_attn.out_proj.bias,
                layer.norm1.weight, layer.norm1.bias, layer.norm2.weight, layer.norm2.bias,
                layer.linear1.weight, layer.linear1.bias, layer.linear2.weight, layer.linear2.bias,
            ):
                add(value)

    add(model.semantic.weight)
    for name, *_shape in DOMAIN_SPECS:
        add_encoder(model.encoders[name])
    add(model.card_state); add(model.card_count.weight)
    add_transformer(model.card_transformer)
    for module in (model.child_tuple, model.child_pool):
        add_linear(module)
    add(model.child_state); add_linear(model.actor_mlp[0]); add_norm(model.actor_mlp[1])
    for module in (model.entity_tuple, model.entity_pool):
        add_linear(module)
    add(model.entity_state); add_norm(model.graph_norm)
    for module in (model.graph_query, model.graph_key_value, model.graph_edge, model.graph_out,
                   model.graph_degree):
        add_linear(module)
    add_norm(model.graph_ff_norm); add_linear(model.graph_ff[0]); add_linear(model.graph_ff[2])
    add(model.graph_global)
    add(model.continuation_relation.weight); add_linear(model.continuation_tuple)
    add_linear(model.continuation_pool); add_linear(model.continuation_parent)
    add_norm(model.continuation_norm)
    add_linear(model.value[0]); add_linear(model.value[2])
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_bytes(b"".join(parts)); temporary.replace(path)
    return sha256_file(path)


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
    if args.precision not in PRECISIONS:
        raise ValueError("precision must be fp32 or bf16")
    if args.precision != "fp32" and target.type not in ("mps", "cuda"):
        raise ValueError(f"{args.precision} requires MPS or CUDA")
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
            inputs = unpack([fit[0][row] for row in index], target, model)
            labels = torch.as_tensor(fit[1][index], device=target)
            loss = nn.functional.binary_cross_entropy_with_logits(
                model(*inputs[:6])[1], labels
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


def probe_candidate_policy(model, target):
    state = torch.randn(1, model.state_width, device=target)
    action = torch.randn(3, model.width, device=target)
    row = torch.zeros(3, dtype=torch.long, device=target)
    legal = torch.ones(3, dtype=torch.bool, device=target)
    sequence = candidate_index(np.zeros(3, np.int32), 1, target)
    base = model.decide(state, action, row, torch.arange(3, device=target), 3, legal, sequence)
    value = model.value(state)
    permutation = torch.tensor((2, 0, 1), device=target)
    permuted = model.decide(state, action[permutation], row, permutation, 3, legal, sequence)
    permuted_value = model.value(state)
    assert torch.allclose(base, permuted, atol=1e-6, rtol=1e-6)
    assert torch.allclose(value, permuted_value, atol=1e-6, rtol=1e-6)
    single_sequence = candidate_index(np.zeros(1, np.int32), 1, target)
    single = model.decide(state, action[:1], row[:1], row[:1], 1, legal[:1], single_sequence)
    assert single.item() == 0


def probe():
    torch.manual_seed(7); np.random.seed(7)
    assert bands([])["mean"] is None
    assert episode_summary([])["floor_mean"] is None
    accelerator = device(); target = torch.device("cpu")
    schedule = argparse.Namespace(
        progress_decisions=5_000_000, progress_beta=None,
        entropy_start=.01, entropy_end=.001,
    )
    legacy = {"decisions": 5_317_231, "auxiliary_decisions": 5_317_231,
              "stage_decisions": 32_768}
    assert resume_stage_decisions(legacy, 0) == 5_317_231
    for stage in range(7):
        beta, entropy, state = curriculum_weights(stage, 0, 5_317_231, schedule, True)
        assert abs(beta - .1) < 1e-7 and entropy == state["entropy_start"]
    assert curriculum_weights(7, 0, 5_317_231, schedule, False)[0] == 0
    try:
        resume_stage_decisions({"decisions": 5_317_231, "auxiliary_decisions": 5_317_231}, 1)
        raise AssertionError("accepted ambiguous legacy stage clock")
    except ValueError:
        pass

    env = sts2_sim.Batch(8, 71, 0)
    layout = dict(env.token_layout())
    assert layout["version"] == FEATURE_VERSION
    assert (layout["model_width"], layout["model_layers"], layout["model_heads"],
            layout["model_feedforward"], layout["card_zones"], layout["entity_summaries"],
            layout["globals"], layout["state_width"], layout["action_width"],
            layout["head_width"]) == (64, 2, 4, 128, 13, 10, 3605, 5141, 64, 112)
    for name, unsigned, signed, semantic, numeric in DOMAIN_SPECS:
        assert tuple(layout[name + suffix] for suffix in ("_u", "_s", "_c", "_f")) == (
            unsigned, signed, semantic, numeric,
        )
    assert tuple(layout["action_" + suffix] for suffix in ("u", "s", "c", "f")) == ACTION_FIELDS
    ranges = [(layout[name + "_semantic_start"], layout[name + "_semantic_count"])
              for name in SEMANTIC_NAMES]
    assert ranges[0][0] == 1 and all(
        start + count == next_start for (start, count), (next_start, _) in zip(ranges, ranges[1:])
    )
    assert ranges[-1][0] + ranges[-1][1] == layout["semantic_vocab"]

    model = Agent(layout).eval()
    assert model.state_width == 5141 and not hasattr(model, "fusion_transformer")
    probe_candidate_policy(model, target)
    observation = env.observe_tokens()
    flat_observation = env.observe_tokens(flat=True)
    assert np.asarray(observation[1]).shape == (8, 3605)
    python_rows, rust_rows = pack_batch(observation), pack_batch(flat_observation)
    assert all(int(left[0]) == int(right[0]) and all(
        np.array_equal(np.asarray(a), np.asarray(b)) for a, b in zip(left[1:], right[1:])
    ) for left, right in zip(python_rows, rust_rows))

    active = np.arange(8) % 2 == 0
    mixed = env.observe_tokens(active.tolist())
    mixed_rows = pack_batch(mixed)
    mixed_inputs = unpack([mixed_rows[index] for index in np.flatnonzero(active)], target, model)
    with torch.no_grad():
        assert len(predict(model, mixed_inputs, "fp32")[0]) == int(active.sum())

    row = python_rows[0]
    represented = np.asarray(observation[3][4], bool)
    action_u, action_s, _action_c, action_f = (
        np.ascontiguousarray(np.asarray(value)[represented]) for value in observation[3][:4]
    )
    sts2_sim.validate_action_features(action_u, action_s, action_f)
    corrupted_f = action_f.copy(); corrupted_f[0, 0] += .125
    try:
        sts2_sim.validate_action_features(action_u, action_s, corrupted_f)
        raise AssertionError("accepted inconsistent action auxiliaries")
    except ValueError:
        pass
    for field in (1, 3, 4):
        corrupted = list(row); corrupted[field] = np.array(row[field], copy=True)
        words = corrupted[field].view(np.uint32)
        words.flat[0] ^= np.uint32(1)
        for operation in (lambda row=tuple(corrupted): validate_packed(row),
                          lambda row=tuple(corrupted): unpack([row], target, model)):
            try:
                operation(); raise AssertionError("accepted corrupted packed observation")
            except ValueError:
                pass

    segment = {
        "rows": [row, row], "choices": [0, 0], "old_log": [0., 0.],
        "values": [.2, .2], "progress_values": [.3, .3], "progress_floors": [.1, .1],
        "win_rewards": [0., 0], "progress_rewards": [0., 0],
        "terminals": [False, False], "characters": [0, 0], "versions": [3, 3],
        "bootstrap_value": .25, "bootstrap_progress": .2, "bootstrap_version": 3,
    }
    assert ExperienceDataset().add(
        {"trajectories": [segment]}, argparse.Namespace(gae_lambda=1., progress_gamma=1.), model,
    ) == 2
    mismatched = copy.deepcopy(segment); mismatched["bootstrap_version"] = 4
    try:
        ExperienceDataset().add(
            {"trajectories": [mismatched]},
            argparse.Namespace(gae_lambda=1., progress_gamma=1.), model,
        )
        raise AssertionError("accepted mixed-version bootstrap")
    except ValueError:
        pass

    inputs = tensors(observation, target, model)
    card_counts = inputs[3][0][-1]
    assert card_counts.shape[1] == 6
    assert torch.allclose(card_counts[:, 1], card_counts[:, 0].log1p())
    assert torch.allclose(card_counts[:, 3], card_counts[:, 2].log1p())
    assert torch.allclose(card_counts[:, 5], card_counts[:, 4].log1p())
    codes = torch.zeros((2, DOMAIN_SPECS[DOMAIN["phase"]][3]), dtype=torch.long)
    codes[:, 0] = torch.tensor((layout["phase_semantic_start"],
                                layout["phase_semantic_start"] + 1))
    numeric = torch.zeros((2, DOMAIN_SPECS[DOMAIN["phase"]][4]))
    identity = model.encoders["phase"](codes, numeric, model.semantic)
    assert not torch.allclose(identity[:1], identity[1:])
    numeric[1, 0] = 1 / 128
    assert not torch.allclose(
        model.encoders["phase"](codes[:1], numeric[:1], model.semantic),
        model.encoders["phase"](codes[:1], numeric[1:], model.semantic),
    )
    gradient = torch.autograd.grad(identity.sum(), model.encoders["phase"].field)[0]
    assert gradient[0].abs().sum() > 0

    position = layout["position_semantic_start"] + 1
    order = layout["order_kind_semantic_start"]
    card_position = torch.zeros((2, DOMAIN_SPECS[DOMAIN["card"]][3]), dtype=torch.long)
    card_position[:, layout["card_known_position_semantic"]] = position
    card_position[:, 2] = torch.tensor((order + 1, order + 2))
    card_position = model.encoders["card"](
        card_position, torch.zeros((2, DOMAIN_SPECS[DOMAIN["card"]][4])), model.semantic,
    )
    assert not torch.allclose(card_position[:1], card_position[1:])
    for name, slots in (
        ("map_node", (layout["map_node_floor_semantic"], layout["map_node_lane_semantic"])),
        ("continuation", tuple(layout[f"continuation_{role}_semantic"]
                               for role in ("branch", "path", "list", "order"))),
    ):
        semantic = torch.zeros((len(slots), DOMAIN_SPECS[DOMAIN[name]][3]), dtype=torch.long)
        semantic[torch.arange(len(slots)), torch.tensor(slots)] = position
        encoded_position = model.encoders[name](
            semantic, torch.zeros((len(slots), DOMAIN_SPECS[DOMAIN[name]][4])), model.semantic,
        )
        assert all(not torch.allclose(encoded_position[left:left + 1], encoded_position[right:right + 1])
                   for left in range(len(slots)) for right in range(left + 1, len(slots)))

    base = torch.randn(4, model.width)
    relation = torch.tensor(((.125, 0, 0, .03125, .015625, 0),
                             (.25, 0, 0, .03125, .015625, 0)))
    def tree(children):
        level = (torch.tensor((0, 1)), torch.tensor(children), torch.tensor((0, 1)),
                 torch.ones(2), relation)
        return model.encode_continuations(base, (torch.arange(4), (level,)))[:2].sum(0)
    assert not torch.allclose(tree((2, 3)), tree((3, 2)))

    with torch.no_grad():
        model.value[-1].weight.normal_(std=.1); model.value[-1].bias.fill_(.03)
        model.progress_value[-1].weight.normal_(std=.1); model.progress_value[-1].bias.fill_(-.02)
        full = model(*inputs[:6], return_state=True)
        output, state = full[:3], full[3]
        assert torch.equal(state[:, :layout["globals"]], inputs[1].float())
        assert state.shape == (8, 5141)
        assert torch.equal(output[0], predict(model, inputs, "fp32", policy_only=True))
        flat_output = predict(model, tensors(flat_observation, target, model), "fp32")
        replay_output = predict(model, unpack([row, row], target, model), "fp32")
    flat_error = [float((left - right).abs().max()) for left, right in zip(output, flat_output)]
    assert max(flat_error) <= 1e-5, flat_error
    assert all(torch.allclose(value[:1], restored[:1], atol=1e-5, rtol=1e-5)
               for value, restored in zip(output, replay_output))

    count = represented.shape[1]
    permutation = np.arange(count)[::-1]; inverse = np.argsort(permutation)
    candidate_domains = []
    for values in observation[2]:
        u, s, c, f, scope, mask = (np.array(value, copy=True) for value in values)
        selected = (scope >= 0) & mask.astype(bool)
        scope[selected] = inverse[scope[selected]]
        candidate_domains.append((u, s, c, f, scope, mask))
    candidate_observation = (
        np.array(observation[0], copy=True), np.array(observation[1], copy=True),
        tuple(candidate_domains),
        tuple(np.asarray(value)[:, permutation].copy() for value in observation[3]),
    )
    with torch.no_grad():
        permuted = predict(model, _tensors(candidate_observation, target, model), "fp32")
    assert torch.allclose(output[0], permuted[0][:, inverse], atol=1e-5, rtol=1e-5)
    assert torch.allclose(output[1], permuted[1], atol=1e-5, rtol=1e-5)

    candidate_batch = next(index for index, mask in enumerate(represented) if mask.sum() > 1)
    candidate = int(np.flatnonzero(represented[candidate_batch])[0])
    illegal = copy.deepcopy(observation[:4])
    illegal_actions = [np.array(value, copy=True) for value in illegal[3]]
    illegal_actions[5][candidate_batch, candidate] = 0
    illegal = (illegal[0], illegal[1], illegal[2], tuple(illegal_actions))
    absent_domains = []
    for values in illegal[2]:
        u, s, c, f, scope, mask = (np.array(value, copy=True) for value in values)
        mask[candidate_batch, scope[candidate_batch] == candidate] = 0
        absent_domains.append((u, s, c, f, scope, mask))
    absent_actions = [np.array(value, copy=True) for value in illegal[3]]
    absent_actions[4][candidate_batch, candidate] = 0
    absent = (illegal[0], illegal[1], tuple(absent_domains), tuple(absent_actions))
    padded = copy.deepcopy(absent)
    padded_actions = [np.array(value, copy=True) for value in padded[3]]
    padded_actions[0][candidate_batch, candidate] = np.iinfo(np.uint32).max
    padded_actions[1][candidate_batch, candidate] = np.iinfo(np.int32).min
    padded_actions[2][candidate_batch, candidate] = np.iinfo(np.uint32).max
    padded_actions[3][candidate_batch, candidate] = np.inf
    padded = (padded[0], padded[1], padded[2], tuple(padded_actions))
    with torch.no_grad():
        illegal_output = predict(model, _tensors(illegal, target, model), "fp32")
        absent_output = predict(model, _tensors(absent, target, model), "fp32")
        padded_output = predict(model, _tensors(padded, target, model), "fp32")
    assert all(torch.allclose(left, right, atol=1e-6, rtol=1e-6)
               for left, right in zip(illegal_output, absent_output))
    assert all(torch.allclose(left, right, atol=1e-6, rtol=1e-6)
               for left, right in zip(absent_output, padded_output))

    relic = [np.array(value, copy=True) for value in observation[2][DOMAIN["relic"]]]
    for value in relic:
        value[:] = value[:, ::-1]
    domains = list(observation[2]); domains[DOMAIN["relic"]] = tuple(relic)
    with torch.no_grad():
        unordered = predict(model, _tensors(
            (observation[0], observation[1], tuple(domains), observation[3]), target, model,
        ), "fp32")
    assert all(torch.allclose(left, right, atol=1e-5, rtol=1e-5)
               for left, right in zip(output, unordered))

    target_env = sts2_sim.Batch(4, 911, 0)
    for _ in range(256):
        target_observation = target_env.observe_tokens()
        target_u = np.asarray(target_observation[3][0])
        target_represented = np.asarray(target_observation[3][4], bool)
        target_legal = np.asarray(target_observation[3][5], bool)
        target_present = target_represented & (target_u[:, :, 1] != 0) & (
            target_u[:, :, 1] != np.iinfo(np.uint32).max
        )
        if target_present.any():
            break
        target_env.step(target_legal.argmax(1).tolist())
    assert target_present.any()
    target_inputs = tensors(target_observation, target, model)
    assert np.array_equal(target_inputs[5][-1].numpy() >= 0, target_present[target_represented])
    encoded = model.encode_domains(target_inputs[2])
    _state, nodes, actors, continuations = model.encode_state(
        target_inputs[1], target_inputs[2], encoded, target_inputs[3],
    )
    joined = model.encode_actions(
        encoded, continuations, actors, target_inputs[4], target_inputs[5], nodes,
    )[0]
    detached_index = (*target_inputs[5][:-1], torch.full_like(target_inputs[5][-1], -1))
    detached = model.encode_actions(
        encoded, continuations, actors, target_inputs[4], detached_index, nodes,
    )[0]
    selected = target_inputs[5][-1] >= 0
    assert not torch.allclose(joined[selected], detached[selected])
    if (~selected).any():
        assert torch.allclose(joined[~selected], detached[~selected])

    dense = copy.deepcopy(model).cpu()
    sequence_row = np.repeat(np.arange(4), (0, 1, 2, 16))
    sequence = sequence_index(sequence_row, 4, target)
    values = torch.randn(len(sequence_row), model.width, requires_grad=True)
    starts = torch.randn(4, model.width, requires_grad=True)
    query = dense.encode_sequence(values, sequence, starts, dense.card_transformer, True)[0]
    query_grad = torch.autograd.grad(query.sum(), (values, starts), retain_graph=True)
    complete = dense.encode_sequence(values, sequence, starts, dense.card_transformer)[0]
    complete_grad = torch.autograd.grad(complete.sum(), (values, starts))
    assert torch.allclose(query, complete, atol=1e-5, rtol=1e-5)
    assert all(torch.allclose(left, right, atol=1e-5, rtol=1e-5)
               for left, right in zip(query_grad, complete_grad))

    full_critic = copy.deepcopy(model).train(); selected_critic = copy.deepcopy(model).train()
    full_inputs = tensors(observation, target, full_critic)
    selected_inputs = tensors(observation, target, selected_critic)
    full_loss = sum(value.square().mean() for value in predict(full_critic, full_inputs, "fp32")[1:])
    selected_loss = sum(
        value.square().mean() for value in predict(selected_critic, selected_inputs, "fp32")[1:]
    )
    full_loss.backward()
    selected_parameters = tuple(selected_critic.value.parameters()) \
        + tuple(selected_critic.progress_value.parameters())
    selected_loss.backward(inputs=selected_parameters)
    full_parameters = dict(full_critic.named_parameters())
    selected_ids = {id(parameter) for parameter in selected_parameters}
    assert all(torch.equal(parameter.grad, full_parameters[name].grad)
               for name, parameter in selected_critic.named_parameters()
               if id(parameter) in selected_ids)
    assert all(parameter.grad is None for parameter in selected_critic.parameters()
               if id(parameter) not in selected_ids)

    trusted = copy.deepcopy(model).train()
    trusted_inputs = tensors(observation, target, trusted)
    logits, value, progress = predict(trusted, trusted_inputs, "fp32")
    loss = logits.masked_fill(~trusted_inputs[6], 0).sum() + value.sum() + progress.sum()
    loss.backward()
    assert all(parameter.grad is None or parameter.grad.isfinite().all()
               for parameter in trusted.parameters())
    if accelerator.type in ("mps", "cuda"):
        mixed = copy.deepcopy(model).to(accelerator).train()
        mixed_inputs = tensors(observation, accelerator, mixed)
        logits, value, progress = predict(mixed, mixed_inputs, "bf16")
        loss = logits.masked_fill(~mixed_inputs[6], 0).sum() + value.sum() + progress.sum()
        loss.backward()
        assert loss.isfinite() and all(
            parameter.grad is None or parameter.grad.isfinite().all()
            for parameter in mixed.parameters()
        )
        actor = Agent(mixed.layout, mixed.width, mixed.layers, mixed.heads,
                      mixed.feedforward, mixed.head_width).cpu().eval()
        actor.load_state_dict({key: value.detach().cpu() for key, value in mixed.state_dict().items()})
        pickle.dumps(actor)

    cached = copy.deepcopy(model).cpu().eval()
    cached_inputs = unpack([row, row], target, cached)
    with torch.no_grad():
        cached_output = predict(cached, cached_inputs, "fp32")
        repeated_output = predict(cached, cached_inputs, "fp32")
    assert cached.cache_stats["card_hit"] and cached.cache_stats["graph_hit"]
    assert all(torch.equal(left, right) for left, right in zip(cached_output, repeated_output))

    parity_env = sts2_sim.Batch(8, 72, 0)
    parity_inputs = tensors(parity_env.observe_tokens(), target, model)
    with torch.no_grad():
        python_value = (predict(model, parity_inputs, "fp32")[1] / .83 - .17).sigmoid().numpy()
    with tempfile.TemporaryDirectory() as directory:
        exported = Path(directory) / "value.bin"
        export_value_model(exported, model, parity_env.fingerprint(), .83, -.17)
        rust_value = np.asarray(parity_env.rust_values(str(exported)))
    parity_error = float(np.max(np.abs(rust_value - python_value)))
    assert parity_error <= 1e-5, parity_error
    parameters = sum(parameter.numel() for parameter in model.parameters())
    assert 2_000_000 <= parameters <= 2_300_000
    print(json.dumps({
        "device": str(accelerator), "layout": layout, "parameters": parameters,
        "state_width": model.state_width,
        "candidate_permutation_error": max(
            float((output[0] - permuted[0][:, inverse]).abs().max()),
            float((output[1] - permuted[1]).abs().max()),
        ),
        "rust_parity_error": parity_error,
    }))


def parser():
    root = argparse.ArgumentParser()
    commands = root.add_subparsers(dest="command", required=True)
    run = commands.add_parser("train")
    run.add_argument("--output", default="target/v61")
    run.add_argument("--checkpoint")
    run.add_argument("--width", type=int)
    run.add_argument("--layers", type=int)
    run.add_argument("--heads", type=int)
    run.add_argument("--feedforward", type=int)
    run.add_argument("--precision", choices=PRECISIONS, default="bf16")
    run.add_argument("--hours", type=float, default=0)
    run.add_argument("--decisions", type=int, default=0)
    run.add_argument("--envs", type=int, default=512)
    run.add_argument("--samplers", type=int, default=4)
    run.add_argument("--torch-threads", type=int, default=2)
    run.add_argument("--sampler-threads", type=int, default=1)
    run.add_argument("--sampler-steps", type=int, default=4)
    run.add_argument("--sampler-timeout", type=float, default=120)
    run.add_argument("--sampler-restarts", type=int, default=3)
    run.add_argument("--segment-steps", type=int, default=0)
    run.add_argument("--publish-updates", type=int, default=4)
    run.add_argument("--report-decisions", type=int, default=32_768)
    run.add_argument("--save-decisions", type=int, default=262_144)
    run.add_argument("--max-policy-lag", type=int, default=128)
    run.add_argument("--max-log-ratio", type=float, default=.5)
    run.add_argument("--policy-temperature", type=float, default=.8)
    run.add_argument("--batch", type=int, default=1024)
    run.add_argument("--learning-rate", type=float, default=3e-4)
    run.add_argument("--gae-lambda", type=float, default=1.0)
    run.add_argument("--progress-gamma", type=float, default=1.0)
    run.add_argument("--progress-decisions", type=int, default=5_000_000)
    run.add_argument("--progress-beta", type=float)
    run.add_argument("--clip", type=float, default=0.2)
    run.add_argument("--value-weight", type=float, default=0.5)
    run.add_argument("--winning-loss-weight", type=float, default=0.1)
    run.add_argument("--entropy-start", type=float, default=0.01)
    run.add_argument("--entropy-end", type=float, default=0.001)
    run.add_argument("--target-kl", type=float, default=0.004)
    run.add_argument("--training-seed", type=int, default=1_900_000_000)
    run.add_argument("--development-seed", type=int, default=3_500_000_000)
    run.add_argument("--development-runs", type=int, default=32)
    run.add_argument("--promotion-seed", type=int, default=3_700_000_000)
    run.add_argument("--promotion-window", type=int, default=128)
    run.add_argument("--promotion-trigger-rate", type=float, default=.12)
    run.add_argument("--promotion-runs", type=int, default=256)
    run.add_argument("--promote-win-rate", type=float, default=0.2)
    run.add_argument("--evaluation-batch", type=int, default=32)
    run.add_argument("--evaluation-max-steps", type=int, default=2048)
    run.add_argument("--evaluation-max-combat-steps", type=int, default=512)
    run.add_argument("--max-steps", type=int, default=2048)
    run.add_argument("--max-combat-steps", type=int, default=512)
    run.add_argument("--seed", type=int, default=1)
    run.add_argument("--promote-now", action="store_true")
    evaluate_parser = commands.add_parser("evaluate")
    evaluate_parser.add_argument("checkpoint")
    evaluate_parser.add_argument("--ascension", type=int, default=0)
    evaluate_parser.add_argument("--bonus", type=int, default=24)
    evaluate_parser.add_argument("--validation-seed", type=int, default=3_500_000_000)
    evaluate_parser.add_argument("--runs", type=int, default=128)
    evaluate_parser.add_argument("--evaluation-batch", type=int, default=32)
    evaluate_parser.add_argument("--max-steps", type=int, default=2048)
    evaluate_parser.add_argument("--max-combat-steps", type=int, default=512)
    evaluate_parser.add_argument("--precision", choices=PRECISIONS, default="fp32")
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
    final.add_argument("--precision", choices=PRECISIONS)
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
