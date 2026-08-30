import argparse
import copy
import ctypes
import fcntl
import hashlib
import json
import math
import multiprocessing
import os
import pickle
import struct
import tempfile
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from queue import Empty, Full, Queue

import numpy as np
import torch
from torch import nn

import sts2_sim


FEATURE_VERSION = 55
MODEL_VERSION = 68
PRECISIONS = ("fp32", "bf16")
WINNING_CAPACITY = 0
CHANGE = "V68: native actors and compact transport with full-model updates."
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

kernel void semantic_forward(
    device const int *semantic, device const float *numeric, device const float *embedding,
    device const float *field, device const float *numeric_weight, device const float *bias,
    device const float *gamma, device const float *beta, device float *input,
    device float *output, device float *stats, constant uint& rows, constant uint& fields,
    constant uint& numeric_width, constant uint& width,
    uint tid [[thread_position_in_grid]], uint lane [[thread_index_in_simdgroup]]) {
    uint row = tid / 32, base = row * width + lane, present = 0;
    float2 value = 0.0f;
    for (uint position = 0; position < fields; ++position) {
        int id = semantic[row * fields + position];
        if (id) {
            present += 1;
            value[0] += embedding[id * width + lane] * field[position * width + lane];
            value[1] += embedding[id * width + lane + 32] * field[position * width + lane + 32];
        }
    }
    float scale = rsqrt(float(max(present, 1u)));
    value *= scale;
    value += float2(bias[lane], bias[lane + 32]);
    for (uint feature = 0; feature < numeric_width; ++feature) {
        float number = numeric[row * numeric_width + feature];
        value[0] += number * numeric_weight[lane * numeric_width + feature];
        value[1] += number * numeric_weight[(lane + 32) * numeric_width + feature];
    }
    float sum = simd_sum(value[0] + value[1]);
    float square = simd_sum(value[0] * value[0] + value[1] * value[1]);
    float mean = sum / width;
    float inverse_std = rsqrt(max(0.0f, square / width - mean * mean) + 1e-5f);
    input[base] = value[0];
    input[base + 32] = value[1];
    output[base] = max(0.0f, (value[0] - mean) * inverse_std * gamma[lane] + beta[lane]);
    output[base + 32] = max(0.0f, (value[1] - mean) * inverse_std * gamma[lane + 32] + beta[lane + 32]);
    if (lane == 0) {
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
        grad_numeric = grad_input[numeric_index].T @ numeric.to(grad_input.dtype)
        return None, None, None, None, None, grad_embedding, grad_numeric, grad_gamma, grad_beta


class _FusedSemantic(torch.autograd.Function):
    @staticmethod
    def forward(ctx, semantic, numeric, embedding, field, numeric_weight, bias, gamma, beta):
        if _RaggedAttention.library is None:
            _RaggedAttention.library = torch.mps.compile_shader(_TRAINING_METAL)
        rows, fields = semantic.shape
        width, numeric_width = numeric_weight.shape
        input = embedding.new_empty((rows, width))
        output = torch.empty_like(input)
        stats = embedding.new_empty((rows, 2))
        _RaggedAttention.library.semantic_forward(
            semantic, numeric, embedding, field, numeric_weight, bias, gamma, beta,
            input, output, stats, rows, fields, numeric_width, width,
            threads=rows * 32, group_size=32,
        )
        ctx.save_for_backward(
            semantic, numeric, embedding, field, numeric_weight, gamma, beta, input, stats,
        )
        ctx.width = width
        return output

    @staticmethod
    def backward(ctx, grad_output):
        semantic, numeric, embedding, field, numeric_weight, gamma, beta, input, stats = ctx.saved_tensors
        grad_output = grad_output.contiguous()
        grad_input = torch.empty_like(input)
        _RaggedAttention.library.token_backward_input(
            input, gamma, beta, stats, grad_output, grad_input,
            ctx.width, threads=len(input) * 32, group_size=32,
        )
        blocks = (len(input) + 127) // 128
        partial = torch.empty((blocks, 2, ctx.width), dtype=torch.float32, device=input.device)
        grad_gamma = torch.empty_like(gamma)
        grad_beta = torch.empty_like(beta)
        _RaggedAttention.library.token_backward_norm_partial(
            input, gamma, beta, stats, grad_output, partial, len(input), 128, ctx.width,
            threads=blocks * ctx.width, group_size=ctx.width,
        )
        _RaggedAttention.library.token_backward_norm_reduce(
            partial, grad_gamma, grad_beta, blocks, ctx.width,
            threads=ctx.width * 128, group_size=128,
        )
        present = semantic != 0
        count = present.sum(1).clamp_min(1).sqrt().to(grad_input.dtype)
        ids = semantic.long()
        scaled = grad_input[:, None] / count[:, None, None]
        mask = present[:, :, None]
        grad_embedding = torch.zeros_like(embedding).index_add(
            0, ids.reshape(-1), (scaled * field * mask).reshape(-1, ctx.width),
        )
        grad_field = (scaled * nn.functional.embedding(ids, embedding) * mask).sum(0)
        return (
            None, None, grad_embedding, grad_field, grad_input.T @ numeric.to(grad_input.dtype),
            grad_input.sum(0), grad_gamma, grad_beta,
        )


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
ACTION_FIELDS = (15, 12, 8, 20)
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
        if semantic.device.type == "mps":
            return _FusedSemantic.apply(
                semantic, numeric, embedding.weight, self.field, self.numeric.weight,
                self.bias, self.norm.weight, self.norm.bias,
            )
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
                     "action_f": ACTION_FIELDS[3], "card_zones": 5, "globals": 0,
                     "entity_summaries": 11, "base_state_width": 1088, "state_width": 1152,
                     "model_width": 64, "model_layers": 2, "model_heads": 4,
                     "model_feedforward": 128, "action_width": 64, "head_width": 112,
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
        self.card_zones = 5
        self.entity_collections = 11
        self.base_state_width = (self.card_zones + self.entity_collections + 1) * width
        self.state_width = self.base_state_width + width
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
        self.effect_tuple = nn.Linear(width, width)
        self.history_tuple = nn.Linear(width, width)
        self.actor_pool = nn.Linear(width + 2, width)
        self.actor_state = nn.Parameter(torch.randn(2, width) * .02)
        self.actor_mlp = nn.Sequential(nn.Linear(3 * width, width), nn.LayerNorm(width), nn.ReLU())
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
        self.menu_object = nn.Linear(width + 2, width)
        self.menu_query = nn.Linear(self.base_state_width, width)
        self.menu_key_value = nn.Linear(width, 2 * width)
        self.menu_out = nn.Linear(width, width)
        self.menu_count = nn.Linear(2, width, bias=False)
        self.menu_norm = nn.LayerNorm(width)
        self.menu_empty = nn.Parameter(torch.randn(width) * .02)
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
        self._graph_cache = {}
        self.cache_stats = {"card_hit": 0, "card_miss": 0, "graph_hit": 0, "graph_miss": 0}
        nn.init.normal_(self.policy[-1].weight, std=0.01)
        nn.init.zeros_(self.policy[-1].bias)
        for head in (self.value, self.progress_value):
            nn.init.zeros_(head[-1].weight)
            nn.init.zeros_(head[-1].bias)

    def load_state_dict(self, state_dict, strict=True, assign=False):
        self._graph_cache.clear()
        self.cache_stats = dict.fromkeys(self.cache_stats, 0)
        return super().load_state_dict(state_dict, strict, assign=assign)

    def train(self, mode=True):
        if mode:
            self._graph_cache.clear()
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

    def pool(self, encoded, source, group, count, groups, state, tuple_linear, pool_linear,
             source_count=None):
        if not groups:
            return encoded.new_empty((0, self.width))
        if len(source):
            values = encoded[source] if tuple_linear is None else torch.relu(tuple_linear(encoded[source]))
            if source_count is not None and len(source) > source_count:
                values = values.clone(); values[source_count:] = 0
            pooled = encoded.new_zeros((len(count), self.width)).index_add(
                0, group, values.to(encoded.dtype),
            )
        else:
            pooled = encoded.new_zeros((len(count), self.width))
        pooled = pooled / count.clamp_min(1).sqrt()[:, None]
        exact = count[:, None].to(pooled.dtype)
        pooled = pool_linear(torch.cat((pooled, exact / 64, exact.log1p() / 5), 1))
        return torch.relu(pooled + state.to(pooled.dtype))[:groups]

    def encode_cards(self, encoded, cards):
        source, sequence, zones, inverse, source_count, counts = cards
        states = self.card_state[zones] + self.card_count(counts.to(encoded.dtype))
        values = encoded[source]
        if len(source) > source_count:
            values = values.clone(); values[source_count:] = 0
        return self.encode_sequence(values, sequence, states, self.card_transformer, True)[0][inverse]

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

    def score(self, state, action, row):
        return self.policy(torch.cat((state[row], action), 1)).squeeze(-1)

    def decide(self, state, action, action_row, action_flat, actions, legal, sequence,
               action_count, temperature=1, flat=False):
        scores = self.group_log_softmax(
            self.score(state, action, action_row)[:action_count] / temperature,
            legal[:action_count], sequence,
        )
        if flat:
            return scores
        policy = scores.new_zeros(len(state) * actions).scatter(
            0, action_flat[:action_count], scores,
        ).reshape(len(state), actions)
        return policy

    def encode_map(self, encoded, index):
        node_source, edge_source, levels, offsets, current, current_inverse, current_source, current_sequence, keys = index
        cached = [self._graph_cache.get(key) for key in keys]
        if not self.training and node_source.device.type == "cpu" and all(value is not None for value in cached):
            self.cache_stats["graph_hit"] += len(keys)
            nodes = torch.cat(cached)
        else:
            if not self.training and node_source.device.type == "cpu":
                self.cache_stats["graph_hit"] += sum(value is not None for value in cached)
                self.cache_stats["graph_miss"] += sum(value is None for value in cached)
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
                    + self.graph_edge(edges[edge_rows]),
                    sequence,
                )
                features = torch.stack((degree / 8, degree.log1p() / 3), 1).to(nodes.dtype)
                updated = nodes[parents] + self.graph_out(attended) + self.graph_degree(features)
                updated = updated + self.graph_ff(self.graph_ff_norm(updated))
                nodes.index_copy_(0, parents[:count], updated[:count])
            if not self.training and node_source.device.type == "cpu":
                offsets = offsets.tolist()
                for index, key in enumerate(keys):
                    if cached[index] is None:
                        self._graph_cache[key] = nodes[offsets[index]:offsets[index + 1]].detach()
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
            pooled = self.continuation_pool(torch.cat((pooled, exact / 64, exact.log1p() / 5), 1))
            parent = self.continuation_parent(torch.cat((values[parents], pooled), 1))
            values = values.index_copy(
                0, parents, torch.relu(self.continuation_norm(parent)).to(values.dtype),
            )
        return values

    def encode_state(self, globals_, domains, encoded, index):
        cards, actors, entities, continuation_index, continuation_roots, map_ = index
        card = self.encode_cards(encoded[DOMAIN["card"]], cards)
        base_source, actor_groups, history, effects = actors
        history_domain, history_source, history_group, history_count, history_source_count = history
        history = self.pool(
            encoded[history_domain], history_source, history_group, history_count, len(base_source),
            self.actor_state[0], self.history_tuple, self.actor_pool, history_source_count,
        )
        effect_values, effect_groups = [], []
        effect_count = None
        for domain, source, group, count, source_count in effects:
            if len(source):
                values = torch.relu(self.effect_tuple(encoded[domain][source]))
                if len(source) > source_count:
                    values = values.clone(); values[source_count:] = 0
                effect_values.append(values)
                effect_groups.append(group)
            effect_count = count if effect_count is None else effect_count + count
        effect_values = torch.cat(effect_values) if effect_values else encoded[0].new_empty((0, self.width))
        effect_groups = torch.cat(effect_groups) if effect_groups else base_source.new_empty(0)
        effects = self.pool(
            effect_values, torch.arange(len(effect_values), device=effect_values.device),
            effect_groups, effect_count, len(base_source), self.actor_state[1], None, self.actor_pool,
        )
        actor = (
            self.actor_mlp(torch.cat((encoded[DOMAIN["actor"]][base_source], history, effects), 1))
            if len(base_source) else encoded[DOMAIN["actor"]].new_empty((0, self.width))
        )
        continuation = self.encode_continuations(
            encoded[DOMAIN["continuation"]], continuation_index,
        )
        current, nodes = self.encode_map(encoded, map_)
        batch = len(globals_)
        summaries = []
        for collection, (domain, source, group, count, source_count) in enumerate(entities[:2]):
            summaries.append(self.pool(
                encoded[domain], source, group, count, batch,
                self.entity_state[collection], self.entity_tuple, self.entity_pool, source_count,
            ))
        for collection, (source, rows, count, source_count) in enumerate(actor_groups, 2):
            summaries.append(self.pool(
                actor, source, rows, count, batch,
                self.entity_state[collection], self.entity_tuple, self.entity_pool, source_count,
            ))
        for collection, (domain, source, group, count, source_count) in enumerate(entities[2:], 4):
            summaries.append(self.pool(
                encoded[domain], source, group, count, batch,
                self.entity_state[collection], self.entity_tuple, self.entity_pool, source_count,
            ))
        roots, root_rows, root_count, root_source_count = continuation_roots
        summaries.append(self.pool(
            continuation, roots, root_rows, root_count, batch,
            self.entity_state[-1], self.entity_tuple, self.entity_pool, root_source_count,
        ))
        card = nn.functional.layer_norm(card, (self.width,))
        summaries = [nn.functional.layer_norm(summary, (self.width,)) for summary in summaries]
        current = nn.functional.layer_norm(current, (self.width,))
        state = torch.cat((card.reshape(batch, -1), *summaries, current), 1)
        if state.shape[1] != self.base_state_width:
            raise ValueError("invalid base state width")
        return state, nodes, actor, continuation

    def encode_actions(self, encoded, continuation, actor, values, index, nodes):
        semantic, numeric = values
        action_row, action_flat, legal, _sequence, actions, sources, path, target, _menu, \
            policy_sequence, action_count = index
        base = self.action_encoder(semantic, numeric, self.semantic)
        pooled = base.new_zeros(base.shape); count = base.new_ones(len(base))
        for domain, (source, group, domain_count, inverse, groups, source_count) in enumerate(sources):
            summed = pooled.new_zeros((groups, self.width))
            if len(source):
                values = continuation[source] if domain == DOMAIN["continuation"] else encoded[domain][source]
                if len(source) > source_count:
                    values = values.clone(); values[source_count:] = 0
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
            exact / 64, exact.log1p() / 5,
        ), 1))
        action = torch.relu(self.action_norm(action))
        return action, action_row, action_flat, actions, legal, policy_sequence, action_count

    def encode_menu(self, state, action, index):
        object_group, target_count, rows, sequence = index
        action = action[:len(object_group)]
        objects = action.new_zeros((len(target_count), self.width)).index_add(0, object_group, action)
        exact = target_count[:, None].to(action.dtype)
        objects = torch.relu(self.menu_object(torch.cat((
            objects / exact.clamp_min(1), exact / 64, exact.log1p() / 5,
        ), 1)))
        menu = self.menu_norm(self.menu_empty.to(state.dtype)).repeat(len(state), 1)
        if len(rows):
            query = self.menu_query(state[rows])
            count = (sequence[0][1:] - sequence[0][:-1])[:, None].to(state.dtype)
            value = self.menu_out(self.attend(query, self.menu_key_value(objects), sequence)) \
                + self.menu_count(torch.cat((count / 64, count.log1p() / 5), 1))
            menu = menu.index_copy(0, rows, self.menu_norm(value).to(menu.dtype))
        return menu

    def forward(self, _character, globals_, domains, state_index, action_values, action_index,
                return_state=False, policy_only=False, flat_policy=False, temperature=1):
        encoded = self.encode_domains(domains)
        state, nodes, actor, continuation = self.encode_state(globals_, domains, encoded, state_index)
        action, action_row, action_flat, actions, legal, sequence, action_count = self.encode_actions(
            encoded, continuation, actor, action_values, action_index, nodes,
        )
        state = torch.cat((state, self.encode_menu(state, action, action_index[8])), 1)
        if state.shape[1] != self.state_width:
            raise ValueError("invalid state width")
        policy = self.decide(
            state, action, action_row, action_flat, actions, legal, sequence, action_count,
            temperature, flat_policy,
        )
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
        "base_state_width": model.base_state_width, "state_width": model.state_width,
        "policy_input": model.state_width + model.width,
        "map_layers": 1, "fusion_layers": 0,
        "final_attention": "query_only",
        "actor_hierarchy": "player+optional Osty party pool; separate enemy pool",
        "actor_effects": "power/status encoders -> one owner-grouped effect pool",
        "tuple_pool": "sum/sqrt(count)+count/64+log1p(count)/5",
        "state_block_norm": "parameterless LayerNorm per 64-d block before concatenation",
        "candidate_attention": "target leaves -> mean action objects -> state-query attention",
        "policy_factorization": "one shared direct candidate scorer",
        "menu_invariance": "explicit object IDs; target means; candidate-permutation invariant",
        "policy_entropy": ["candidate", "normalized", "effective_actions"],
        "map": "reverse-topological sparse attention; current/entry query over all contextual nodes",
        "integer_encoding": "namespaced semantic IDs + field-specific float32 auxiliaries",
        "continuations": "bottom-up parent/branch/path relational pooling",
        "candidate_context": "complete public payload, Path context, and contextual target actor",
        "card_zone_names": ["deck", "hand", "draw", "discard", "exhaust"],
        "state_summaries": ["run", "phase", "party", "enemies", "relics", "potions", "orbs",
                            "events", "encounters", "crystal", "continuations", "current_map_node",
                            "available_actions"],
        "semantic_vocab": model.semantic.num_embeddings, "candidate_numeric": ACTION_FIELDS[3],
        "value_heads": ["win_logit", "progress"],
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


def upload(value, target):
    marker = object()
    tensors = []

    def flatten(item):
        if torch.is_tensor(item):
            tensors.append(item)
            return marker, len(tensors) - 1
        if isinstance(item, tuple):
            return tuple(flatten(child) for child in item)
        return item

    structure = flatten(value)
    uploaded = [None] * len(tensors)
    for dtype in {tensor.dtype for tensor in tensors}:
        selected = [index for index, tensor in enumerate(tensors) if tensor.dtype == dtype]
        sizes = [tensors[index].numel() for index in selected]
        buffer = torch.cat([tensors[index].reshape(-1) for index in selected]).to(
            target, non_blocking=True,
        )
        offset = 0
        for index, size in zip(selected, sizes):
            uploaded[index] = buffer[offset:offset + size].view(tensors[index].shape)
            offset += size

    def rebuild(item):
        if isinstance(item, tuple) and len(item) == 2 and item[0] is marker:
            return uploaded[item[1]]
        if isinstance(item, tuple):
            return tuple(rebuild(child) for child in item)
        return item

    return rebuild(structure)


def trust_region_step(model, optimizer, inputs, action, old, fresh, denominator, precision, limit,
                      temperature=1, attempts=3, choice_index=None):
    parameters = [parameter for parameter in model.parameters() if parameter.grad is not None]
    nn.utils.clip_grad_norm_(parameters, .5)
    optimizer_state = copy.deepcopy(optimizer.state_dict()) if any(
        parameter.grad is not None and parameter not in optimizer.state for parameter in parameters
    ) else None
    cache = getattr(optimizer, "_trust_region_saved", {})
    key = tuple(map(id, parameters))
    saved = cache.get(key)
    if saved is None:
        saved = [value.detach().clone() for value in parameters]
        cache[key] = saved
        optimizer._trust_region_saved = cache
    else:
        torch._foreach_copy_(saved, parameters)

    def restore():
        with torch.no_grad():
            torch._foreach_copy_(parameters, saved)
        if optimizer_state is not None:
            optimizer.load_state_dict(optimizer_state)
        else:
            with torch.no_grad():
                for parameter in parameters:
                    state = optimizer.state[parameter]
                    beta1, beta2 = optimizer.param_groups[0]["betas"]
                    state["exp_avg"].sub_(parameter.grad, alpha=1 - beta1).div_(beta1)
                    state["exp_avg_sq"].addcmul_(
                        parameter.grad, parameter.grad, value=beta2 - 1,
                    ).div_(beta2)
                    state["step"].sub_(1)

    proposals = []
    try:
        optimizer.step()
        for attempt in range(attempts):
            if attempt:
                with torch.no_grad():
                    for parameter, original in zip(parameters, saved):
                        parameter.sub_(original).mul_(.25).add_(original)
            with torch.no_grad():
                kind = inputs[0].device.type
                dtype = torch.bfloat16 if precision == "bf16" else torch.float16
                with torch.autocast(kind, dtype=dtype, enabled=precision != "fp32"):
                    logits = model(
                        *inputs[:6], policy_only=True, flat_policy=choice_index is not None,
                        temperature=temperature,
                    ).float()
                selected = logits[choice_index] if choice_index is not None else \
                    logits[:len(action)].gather(1, action[:, None]).squeeze(1)
                log_ratio = selected - old
                post_kl = (((log_ratio.exp() - 1 - log_ratio) * fresh).sum() / denominator).float()
            proposals.append(float(post_kl))
            if torch.isfinite(post_kl) and proposals[-1] <= limit:
                return True, proposals, log_ratio
        restore()
        return False, proposals, None
    except Exception:
        restore()
        raise


def critic_only_step(model, optimizer, parameters=None):
    parameters = parameters or (tuple(model.value.parameters()) + tuple(model.progress_value.parameters()))
    selected = {id(parameter) for parameter in parameters}
    for parameter in model.parameters():
        if id(parameter) not in selected:
            parameter.grad = None
    nn.utils.clip_grad_norm_(parameters, .5)
    optimizer.step()


def sequence_index(row, batch, target, pad=False, items=None):
    counts = np.bincount(row, minlength=batch).astype(np.int32)
    lengths = counts + 1
    starts = np.repeat(np.cumsum(counts) - counts, counts)
    position = np.arange(len(row), dtype=np.int32) - starts + 1
    destination = np.repeat(np.cumsum(lengths) - lengths, counts) + position
    if pad:
        padded_batch = (batch // 256 + 1) * 256
        lengths = np.pad(lengths, (0, padded_batch - batch), constant_values=1)
        extra = (-int(lengths.sum())) % 4096
        lengths[batch:] += extra // (padded_batch - batch)
        lengths[batch:batch + extra % (padded_batch - batch)] += 1
    offsets = np.r_[0, np.cumsum(lengths, dtype=np.int32)].astype(np.int32)
    if items is not None and items > len(row):
        extra = items - len(row)
        destination = np.pad(destination, (0, extra), constant_values=offsets[batch])
        row = np.pad(row, (0, extra), constant_values=batch)
        position = np.pad(position, (0, extra))
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
    pad_sequences = model.training and model.card_state.device.type == "mps" and batch <= 8192
    bucket = lambda count: 0 if not count else min(
        size for power in range(max(1, (count - 1).bit_length()), 64)
        for size in (3 * (1 << power) // 4, 1 << power) if size >= count
    )
    tensor = lambda value: torch.as_tensor(value, dtype=torch.long, device=target)
    def grouped(source, group, groups):
        source, group = np.asarray(source, np.int32), np.asarray(group, np.int32)
        count = len(source)
        if pad_sequences and count:
            size = bucket(count)
            source = np.pad(source, (0, size - count))
            group = np.pad(group, (0, size - count))
        counts = np.bincount(group[:count], minlength=bucket(groups) if pad_sequences else groups)
        return (torch.as_tensor(source, device=target), torch.as_tensor(group, device=target),
                torch.as_tensor(counts.astype(np.float32), device=target), count)
    domain_tensors = []
    domain_inverse = []
    for (name, *_), (_u, _s, semantic, numeric, _row, _scope) in zip(DOMAIN_SPECS, domains):
        if semantic.max(initial=0) >= model.semantic.num_embeddings:
            raise ValueError(f"invalid {name} semantic id")
        if len(semantic):
            first, inverse = map(
                np.asarray, sts2_sim.unique_feature_rows(semantic, numeric.view(np.uint32)),
            )
        else:
            first = inverse = np.empty(0, np.intp)
        count = len(first)
        size = bucket(count) if pad_sequences else count
        domain_tensors.append((
            torch.as_tensor(np.pad(semantic[first], ((0, size - count), (0, 0))).astype(np.int32),
                            device=target),
            torch.as_tensor(np.pad(numeric[first], ((0, size - count), (0, 0))), device=target),
            count,
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
    card_counts = np.stack((card_counts / 64, np.log1p(card_counts) / 5,
                            card_top / 64, np.log1p(card_top) / 5,
                            card_bottom / 64, np.log1p(card_bottom) / 5), 1).astype(np.float32)
    card_counts = np.concatenate((np.zeros((model.card_zones, 6), np.float32), card_counts))[first]
    card_groups = len(first)
    actor_u, _actor_s, _actor_c, _actor_f, actor_rows, _actor_scope = domains[DOMAIN["actor"]]
    base_source = ordered(state_source[DOMAIN["actor"]], actor_rows, actor_u[:, 0])
    actor_keys = (actor_rows[base_source].astype(np.uint64) << 32) | actor_u[base_source, 0]
    if len(actor_keys) != len(np.unique(actor_keys)):
        raise ValueError("duplicate actor")
    children = []
    for domain in (DOMAIN["history"], DOMAIN["power"], DOMAIN["status"]):
        u, _s, _c, _f, row, scope = domains[domain]
        source = state_source[domain]
        keys = (row[source].astype(np.uint64) << 32) | u[source, 0]
        group = np.searchsorted(actor_keys, keys)
        valid = group < len(actor_keys)
        if not valid.all() or np.any(actor_keys[group] != keys):
            raise ValueError("actor child without actor")
        group = group.astype(np.int32)
        order = np.argsort(group, kind="stable")
        source, group = source[order], group[order]
        children.append((domain, *grouped(
            domain_inverse[domain][source], group, len(base_source),
        )))
    entities = []
    for name in ("run", "phase", "relic", "potion", "orb", "event", "encounter", "crystal"):
        domain = DOMAIN[name]; _u, _s, _c, _f, row, _scope = domains[domain]
        source = ordered(state_source[domain], row)
        group = row[source].astype(np.int32)
        entities.append((domain, *grouped(domain_inverse[domain][source], group, batch)))
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
    continuation_roots = roots[cont_scope[cont_source[roots]] < 0]
    continuation_rows = cont_row[cont_source[roots]][cont_scope[cont_source[roots]] < 0]
    node_u, _node_s, node_c, node_f, node_row, _node_scope = domains[DOMAIN["map_node"]]
    edge_u, _edge_s, edge_c, edge_f, edge_row, _edge_scope = domains[DOMAIN["map_edge"]]
    node_source = ordered(state_source[DOMAIN["map_node"]], node_row, node_u[:, 0])
    edge_source = ordered(state_source[DOMAIN["map_edge"]], edge_row, edge_u[:, 0], edge_u[:, 1])
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
    node_offsets = np.r_[0, np.cumsum([len(node_groups[row]) for row in representatives])]
    graph_for_node = np.repeat(np.arange(len(representatives)), np.diff(node_offsets))
    missing_graph = np.asarray([
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
        order = np.argsort(edge_src[selected], kind="stable"); selected = selected[order]
        parents, group = np.unique(edge_src[selected], return_inverse=True)
        selected = selected[missing_graph[graph_for_node[parents]][group]]
        if not len(selected):
            continue
        parents, group = np.unique(edge_src[selected], return_inverse=True)
        levels.append((parents, edge_dst[selected], selected, group,
                       node_u[node_source[parents], 9].astype(np.float32)))
    if pad_sequences and levels:
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
    position = np.where(current >= 0, current, entry)
    current, current_inverse = np.unique(position, return_inverse=True)
    current_graph = np.searchsorted(node_offsets[1:], current, side="right")
    current_count = np.diff(node_offsets)[current_graph]
    current_source = np.concatenate([
        np.arange(node_offsets[graph], node_offsets[graph + 1], dtype=np.int32)
        for graph in current_graph
    ])
    current_group = np.repeat(np.arange(len(current), dtype=np.int32), current_count)
    map_index = (tensor(domain_inverse[DOMAIN["map_node"]][node_source]),
                 tensor(domain_inverse[DOMAIN["map_edge"]][edge_source]), tuple(levels),
                 tensor(node_offsets), tensor(current), tensor(current_inverse), tensor(current_source),
                 candidate_index(current_group, len(current), target),
                 tuple(graph_keys[row] for row in representatives) if graph_keys else ())
    actor_rows = actor_rows[base_source].astype(np.int32)
    actor_kinds = actor_u[base_source, 1].astype(np.int32)
    actor_groups = tuple(grouped(
        source, actor_rows[source], batch,
    ) for source in (np.flatnonzero(actor_kinds < 2), np.flatnonzero(actor_kinds == 2)))
    continuation_roots = grouped(continuation_roots, continuation_rows, batch)
    card_source_count = len(card_source)
    card_size = bucket(card_source_count) if pad_sequences else card_source_count
    state_index = (
        (tensor(np.pad(domain_inverse[DOMAIN["card"]][card_source],
                       (0, card_size - card_source_count))),
         sequence_index(card_group, card_groups, target, pad_sequences, card_size),
         tensor(card_zones), tensor(card_inverse), card_source_count,
         torch.as_tensor(card_counts, device=target)),
        (tensor(domain_inverse[DOMAIN["actor"]][base_source]), actor_groups,
         children[0], tuple(children[1:])),
        tuple(entities), continuation_index, continuation_roots, map_index,
    )
    action_u, action_s, action_c, action_f = action
    action_count = len(action_u)
    policy_sequence = candidate_index(action_row, batch, target)
    action_size = bucket(action_count) if pad_sequences else action_count
    action_width = bucket(actions) if pad_sequences else actions
    candidate_sources = []
    for domain, (u, s, _c, _f, _row, scope) in enumerate(domains):
        if domain == DOMAIN["continuation"]:
            selected = roots[cont_scope[cont_source[roots]] >= 0]
            group = cont_scope[cont_source[selected]]
            source, group, domain_count, source_count = grouped(selected, group, action_size)
            candidate_sources.append((
                source, group, domain_count, tensor(np.arange(action_size, dtype=np.int32)),
                len(domain_count), source_count,
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
            groups = len(unique_source) + 1
            source, group, domain_count, source_count = grouped(
                unique_source, np.arange(1, groups), groups,
            )
            candidate_sources.append((
                source, group, domain_count,
                tensor(np.pad(inverse, (0, action_size - len(inverse)))),
                len(domain_count), source_count,
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
        source, group, domain_count, source_count = grouped(
            domain_inverse[domain][source], group, len(first),
        )
        candidate_sources.append((
            source, group, domain_count,
            tensor(np.pad(inverse, (0, action_size - len(inverse)))),
            len(domain_count), source_count,
        ))
    if action_c.max(initial=0) >= model.semantic.num_embeddings:
        raise ValueError("invalid action semantic id")
    action_c = np.pad(action_c, ((0, action_size - action_count), (0, 0)))
    action_f = np.pad(action_f, ((0, action_size - action_count), (0, 0)))
    action_legal = np.pad(legal[action_row, action_position], (0, action_size - action_count))
    path = np.full(len(action_row), -1, np.int32)
    path_id = action_u[:, 4]
    present = path_id != np.iinfo(np.uint32).max
    if path_id[present].max(initial=0) > max_node:
        raise ValueError("path candidate references missing node")
    path[present] = node_lookup[action_row[present], path_id[present].astype(np.intp)]
    if np.any(path[present] < 0):
        raise ValueError("path candidate references missing node")
    path = np.pad(path, (0, action_size - action_count), constant_values=-1)
    actor_target = np.full(len(action_row), -1, np.int32)
    owner = action_u[:, 1]
    present = (owner != 0) & (owner != np.iinfo(np.uint32).max)
    keys = (action_row[present].astype(np.uint64) << 32) | owner[present]
    group = np.searchsorted(actor_keys, keys)
    valid = group < len(actor_keys)
    if not valid.all() or np.any(actor_keys[group] != keys):
        raise ValueError("candidate target references missing actor")
    actor_target[present] = group
    actor_target = np.pad(actor_target, (0, action_size - action_count), constant_values=-1)
    object_keys = np.column_stack((action_row.astype(np.uint32), action_u[:, 0], action_u[:, 14]))
    object_first, object_group = map(np.asarray, sts2_sim.unique_rows(object_keys))
    object_group = object_group.astype(np.int32)
    object_rows = action_row[object_first].astype(np.int32)
    object_count = np.bincount(object_group, minlength=len(object_rows)).astype(np.float32)
    menu_rows = np.unique(object_rows)
    menu_lookup = np.full(batch, -1, np.int32); menu_lookup[menu_rows] = np.arange(len(menu_rows))
    menu_sequence = candidate_index(menu_lookup[object_rows], len(menu_rows), target, True)
    menu_index = (
        tensor(object_group), torch.as_tensor(object_count, device=target),
        tensor(menu_rows), menu_sequence,
    )
    action_flat = action_row * action_width + action_position
    if action_size > action_count:
        occupied = np.zeros(batch * action_width, bool); occupied[action_flat] = True
        dummy_flat = np.flatnonzero(~occupied)[:action_size - action_count]
        action_row = np.pad(action_row, (0, action_size - action_count))
        action_row[action_count:] = dummy_flat // action_width
        action_flat = np.r_[action_flat, dummy_flat]
    padded_legal = np.pad(legal, ((0, 0), (0, action_width - actions)))
    action_index = tuple(torch.as_tensor(value, device=target) for value in (
        action_row, action_flat, action_legal,
    )) + (candidate_index(action_row, batch, target), action_width, tuple(candidate_sources),
          tensor(path), tensor(actor_target), menu_index, policy_sequence, action_count)
    return (
        torch.as_tensor(character, dtype=torch.long, device=target),
        torch.as_tensor(globals_, device=target), domain_tensors, state_index,
        tuple(torch.as_tensor(value, device=target) for value in (
            action_c.astype(np.int32), action_f.astype(np.float32),
        )), action_index, torch.as_tensor(padded_legal, device=target),
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
        if (digest or globals_.any() or counts.shape != (len(DOMAIN_SPECS),) or counts.any() or len(exact)
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
    return sum(struct.unpack_from(f"<{len(DOMAIN_SPECS)}I", row, 32)) \
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


def pack_batch(observation, validate=True):
    rows = _pack_batch(observation)
    if len(rows) != len(observation[0]):
        raise ValueError("packed observation row count mismatch")
    if validate:
        for row in rows:
            validate_packed(row)
    return rows


def pack(observation, index, model):
    return pack_batch(observation)[index]


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
        if not self.capacity:
            return
        for index, (row, choice, log_probability, policy, value) in enumerate(
            zip(rows, choices, log_probabilities, policies, values)
        ):
            if packed_legal_count(row) <= 1:
                self.forced += 1
                continue
            episode = self.pending[index]
            self.pending_seen[index] += 1
            item = (row, int(choice), float(log_probability), 1.0 - float(value),
                    np.asarray(policy[:packed_action_count(row)], np.float16))
            if len(episode) < self.pending_capacity:
                episode.append(item)
            else:
                slot = int(rng.integers(self.pending_seen[index]))
                if slot < self.pending_capacity:
                    episode[slot] = item

    def finish(self, indices, won, rng):
        if not self.capacity:
            return
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
            character = packed_character(item[0])
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
            character = packed_character(row[0])
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
            character = packed_character(row)
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

    def sample(self, count, rng):
        pool = [index for index, row in enumerate(self.rows) if row[4] is not None]
        selected = rng.choice(pool, min(count, len(pool)), replace=False)
        return [self.rows[index] for index in selected]


def act(model, observation, target, sample, precision, generator=None, temperature=1):
    with torch.inference_mode():
        inputs = tensors(observation, torch.device("cpu") if target.type == "mps" else target, model)
        if target.type == "mps":
            inputs = upload(inputs, target)
        logits, value_logit, progress_value = predict(
            model, inputs, precision, temperature, flat_policy=True,
        )
        action_row = inputs[5][0][:len(logits)]
        action_flat = inputs[5][1][:len(logits)]
        legal = inputs[5][2][:len(logits)]
        masked = logits.masked_fill(~legal, -torch.inf)
        if sample:
            race = torch.empty_like(masked).exponential_(generator=generator).log() - masked
            minimum = race.new_full((len(observation[0]),), torch.inf).scatter_reduce_(
                0, action_row, race, reduce="amin",
            )
            selected = (race == minimum[action_row]).nonzero().squeeze(1)
        else:
            maximum = masked.new_full((len(observation[0]),), -torch.inf).scatter_reduce_(
                0, action_row, masked, reduce="amax",
            )
            selected = (masked == maximum[action_row]).nonzero().squeeze(1)
        choice = action_flat[selected] % inputs[5][4]
        assert len(choice) == len(observation[0]) and legal[selected].all()
    policy = None
    return (choice.cpu().numpy(), masked[selected].cpu().numpy(),
            policy, value_logit.sigmoid().cpu().numpy(),
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


def promotion_sample(episodes, size, threshold):
    recent = episodes[-size:]
    grouped = [[row for character, row in recent if character == value] for value in range(5)]
    result = episode_summary([row for _, row in recent]) | {
        "runs": len(recent),
        "characters": [episode_summary(rows) | {
            "character": character, "runs": len(rows),
        } for character, rows in enumerate(grouped)],
    }
    return len(recent) == size and all(
        rows and sum(row[0] for row in rows) / len(rows) > threshold for rows in grouped
    ), result


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
        self.native_steps = []
        self.native_starts = np.zeros(args.envs, np.int64)
        self.native_started = np.full(args.envs, time.monotonic())

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
        native = model is None
        cache_start = dict.fromkeys(("card_hit", "card_miss", "graph_hit", "graph_miss"), 0) \
            if native else model.cache_stats.copy()
        finished = []
        episodes = [[] for _ in range(5)]
        sample_keys = (
            "rows", "choices", "old_log", "values", "progress_values", "progress_floors",
            "win_rewards", "progress_rewards", "terminals", "characters", "versions",
        )
        def materialize(trajectory):
            samples = trajectory.pop("samples")
            trajectory.update(zip(sample_keys, map(list, zip(*samples))))
            return trajectory
        discarded_steps = orphan_empty_actions = sampled_steps = 0
        collect_seconds = 0.0
        for _ in range(steps):
            if time.monotonic() >= deadline or stop and stop.is_set():
                break
            if heartbeat is not None:
                heartbeat[self.worker] = time.monotonic()
            self.iteration += 1
            empty = np.zeros(args.envs, bool) if native else ~observation_legal(self.observation).any(1)
            if empty.any():
                reset = np.flatnonzero(empty).tolist()
                stale_characters = np.asarray(self.observation[0], np.uint8)
                self.trace_empty("orphan", reset, stale_characters, self.env.stats())
                orphan_empty_actions += len(reset)
                self.reservoir.discard(reset)
                for index in reset:
                    if self.trajectories[index] is not None:
                        discarded_steps += len(self.trajectories[index]["samples"])
                    self.trajectories[index] = None
                    self.action_history[index].clear()
                self.env.reset(reset, 0.0)
                self.episode_steps[reset] = 0
                self.combat_steps[reset] = 0
                self.max_floor[reset] = 1
                self.observation = self.env.observe_tokens(flat=True)
            step_started = time.monotonic()
            if native:
                characters, choice, log_probability, value, progress_value, step_rows = \
                    self.env.policy(args.policy_temperature)
                characters = np.asarray(characters, np.uint8)
                policy = None
            else:
                characters = np.asarray(self.observation[0], np.uint8)
                choice, log_probability, policy, value, progress_value = act(
                    model, self.observation, target, True, precision, self.torch_rng,
                    args.policy_temperature,
                )
                step_rows = _pack_batch(self.observation)
            progress_floor = self.max_floor / 52
            if not native:
                for index, action in enumerate(choice):
                    self.action_history[index].append(int(action))
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
            next_observation = None if native else self.env.observe_tokens((~done).tolist(), True)
            stats = self.env.stats()
            floors = np.asarray([(row[0] - 1) * 17 + row[1] for row in stats], np.float32)
            next_max_floor = np.maximum(np.maximum(self.max_floor, floors), 52 * raw_reward)
            still_combat = np.asarray([row[4] == 1 for row in stats])
            truncated, step_truncated, combat_truncated, empty_actions = cuts(
                done, self.episode_steps, self.combat_steps, still_combat,
                np.asarray(self.env.has_legal_actions((~done).tolist()))[:, None]
                if native else observation_legal(next_observation),
                args.max_steps, args.max_combat_steps,
            )
            boundary = done | truncated
            if empty_actions.any():
                reset = np.flatnonzero(empty_actions).tolist()
                self.trace_empty("post_step", reset, characters, stats)
            progress_reward = (next_max_floor - self.max_floor) / 52
            self.max_floor = next_max_floor
            if native:
                self.native_steps.append((
                    step_rows, choice, log_probability, value, progress_value, progress_floor,
                    raw_reward, progress_reward, done, characters, version,
                ))
            else:
                for index, row in enumerate(step_rows):
                    trajectory = self.trajectories[index]
                    if trajectory is None:
                        trajectory = {"samples": [], "started": step_started}
                        self.trajectories[index] = trajectory
                    trajectory["samples"].append((
                        row, choice[index], log_probability[index], value[index],
                        progress_value[index], progress_floor[index], raw_reward[index],
                        progress_reward[index], done[index], characters[index], version,
                    ))
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
                    if native and done[index]:
                        history = self.native_steps[self.native_starts[index]:]
                        trajectory = {
                            key: [step[column] if column == 10 else step[column][index]
                                  for step in history]
                            for column, key in enumerate(sample_keys)
                        }
                        trajectory["completion_seconds"] = time.monotonic() - self.native_started[index]
                        finished.append(trajectory)
                    elif native:
                        discarded_steps += len(self.native_steps) - int(self.native_starts[index])
                    elif done[index]:
                        trajectory = self.trajectories[index]
                        trajectory["completion_seconds"] = time.monotonic() - trajectory.pop("started")
                        finished.append(materialize(trajectory))
                    else:
                        discarded_steps += len(self.trajectories[index]["samples"])
                    self.trajectories[index] = None
                    self.action_history[index].clear()
                    self.native_starts[index] = len(self.native_steps)
                    self.native_started[index] = time.monotonic()
                self.env.reset(reset, 0.0)
                self.episode_steps[reset] = 0
                self.combat_steps[reset] = 0
                self.max_floor[reset] = 1
                next_observation = None if native else self.env.observe_tokens(flat=True)
                if native:
                    drop = int(self.native_starts.min())
                    if drop:
                        del self.native_steps[:drop]
                        self.native_starts -= drop
            segment = [
                index for index, trajectory in enumerate(self.trajectories)
                if trajectory is not None and args.segment_steps
                and len(trajectory["samples"]) >= args.segment_steps
            ]
            if segment:
                if native:
                    raise ValueError("native actors require complete terminal trajectories")
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
                    finished.append(materialize(trajectory)); self.trajectories[index] = None
            self.observation = next_observation
            collect_seconds += time.monotonic() - step_started
            if finished and not native:
                break
        return {
            "trajectories": finished,
            "episodes": episodes,
            "iteration": self.iteration,
            "orphan_empty_actions": orphan_empty_actions,
            "collect_seconds": collect_seconds, "discarded_steps": discarded_steps,
            "sampled_steps": sampled_steps,
            **({key: 0 for key in cache_start} if native else {
                key: model.cache_stats[key] - cache_start[key] for key in model.cache_stats
            }),
        }

    def run(self, model, models, samples, stop, deadline, budget, worker=0,
            heartbeat=None, progress=None, version=0, target=None, precision="fp32"):
        target = target or torch.device("cpu")
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
        pending_rows = 0
        while produced < budget and time.monotonic() < deadline and not stop.is_set():
            latest = None
            while True:
                try:
                    latest = models.get_nowait()
                except (Empty, EOFError, OSError):
                    break
            if latest:
                version, state = latest
                if model is None:
                    self.env.load_policy(state)
                else:
                    model.load_state_dict(state)
            steps = sampling_steps(produced, budget, self.args.envs, self.args.sampler_steps)
            if not steps:
                break
            result = self.collect(
                model, target, precision, deadline, steps, version, stop,
                heartbeat, progress,
            )
            pending["trajectories"].extend(result["trajectories"])
            for target_episodes, rows in zip(pending["episodes"], result["episodes"]):
                target_episodes.extend(rows)
            for key in (
                "orphan_empty_actions", "collect_seconds",
                "discarded_steps", "sampled_steps", "card_hit", "card_miss",
                "graph_hit", "graph_miss",
            ):
                pending[key] += result[key]
            pending["iteration"] = result["iteration"]
            if not pending["trajectories"]:
                continue
            added = sum(len(trajectory["rows"]) for trajectory in result["trajectories"])
            pending_rows += added
            produced += added
            if pending_rows < self.args.envs * 4 and produced < budget \
                    and time.monotonic() < deadline and not stop.is_set():
                continue
            pending["reservoir"] = self.reservoir.drain_candidates()
            pending["queued_at"] = time.monotonic()
            put_started = pending["queued_at"]
            while not stop.is_set():
                try:
                    pending["queue_put_seconds"] = time.monotonic() - put_started
                    samples.put_nowait((worker, version, pending))
                    pending = empty()
                    pending_rows = 0
                    break
                except Full:
                    pending["queue_full_waits"] += 1
                    if heartbeat is not None:
                        heartbeat[worker] = time.monotonic()
                    time.sleep(.01)
        pending["discarded_steps"] += (
            sum(len(self.native_steps) - int(start) for start in self.native_starts)
            if model is None else
            sum(len(trajectory["samples"]) for trajectory in self.trajectories
                if trajectory is not None)
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
    qos = getattr(ctypes.CDLL(None), "pthread_set_qos_class_self_np", None)
    if qos is not None:
        qos(int(os.environ.get("ACTOR_QOS", "0x11"), 0), 0)
    if args.sampler_backend == "process":
        os.environ["RAYON_NUM_THREADS"] = str(args.sampler_threads)
        torch.set_num_threads(args.torch_threads)
        torch.set_num_interop_threads(1)
        torch._C._set_default_mobile_cpu_allocator()
    reservoir = WinningReservoir(capacity, args.envs, pending_capacity)
    collector = RolloutCollector(
        args, sampler_session, stage, reservoir, iteration, worker, generation,
    )
    target = torch.device("mps") if os.environ.get("ACTOR_MPS") else torch.device("cpu")
    precision = "bf16" if target.type == "mps" else "fp32"
    if target.type == "mps":
        model = model.to(target)
        collector.torch_rng = torch.Generator(device=target).manual_seed(
            args.seed + sampler_session * 10_000_000 + worker + generation * args.samplers
        )
    if isinstance(model, bytes):
        collector.env.load_policy(model)
        collector.observation = None
        model = None
    heartbeat[worker] = time.monotonic()
    results.put(collector.run(
        model, models, samples, stop, deadline, budget, worker, heartbeat, progress, version,
        target, precision,
    ))


class ExperienceDataset:
    def __init__(self):
        self.rows = []
        self.data = {
            "action": np.empty(0, np.int64), "old": np.empty(0, np.float32),
            "advantage": np.empty(0, np.float32), "progress_advantage": np.empty(0, np.float32),
            "returns": np.empty(0, np.float32), "progress_returns": np.empty(0, np.float32),
            "character": np.empty(0, np.int8), "priority": np.empty(0, np.float32),
            "version": np.empty(0, np.int64), "id": np.empty(0, np.int64),
        }
        self.next_id = 0
        self.seen = self.admitted = self.uses = self.retired = self.forced_dropped = 0
        self.stale_dropped = self.ratio_dropped = self.kl_dropped = self.post_kl_dropped = 0

    def __len__(self):
        return len(self.rows)

    def add(self, result, args, progress_active=True):
        trajectories = result["trajectories"]
        if not trajectories:
            return 0, 0
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
        actionable = np.asarray([packed_legal_count(row) > 1 for row in rows])
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
            "id": np.arange(self.next_id, self.next_id + len(rows), dtype=np.int64),
        }
        self.next_id += len(rows)
        self.rows.extend(row for row, keep in zip(rows, actionable) if keep)
        for key, value in values.items():
            self.data[key] = np.concatenate((self.data[key], value[actionable]))
        self.seen += len(rows)
        self.admitted += int(actionable.sum())
        forced = len(rows) - int(actionable.sum())
        self.forced_dropped += forced
        return len(rows), forced

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

    def discard_ids(self, ids):
        indices = np.flatnonzero(np.isin(self.data["id"], ids))
        self.discard(indices)
        return len(indices)

    def sample(self, size, rng):
        size = min(size, len(self))
        return rng.choice(len(self), size, replace=False)

    def use(self, indices):
        self.uses += len(indices)
        self.data["priority"][indices] -= 3
        expired = indices[self.data["priority"][indices] < 0]
        self.retired += len(expired)
        return expired


def train_stream(model, optimizer, args, sampler_session, stage, target, deadline, budget,
                 base_decisions, auxiliary_decisions, stage_decisions, run_started, reservoir, progress_active,
                 save_report, save_step, fingerprint):
    ascension, bonus = STAGES[stage]
    collector_args = copy.copy(args)
    pending_capacity = max(1, reservoir.capacity // args.envs)
    collector_args.envs //= args.samplers
    dataset = ExperienceDataset()
    episodes = [[] for _ in range(5)]
    promotion_episodes = []
    losses = {key: [] for key in ("mean_advantage", "policy_loss", "value_loss", "progress_value_loss", "progress_beta", "entropy", "entropy_weight", "kl", "post_kl", "clip_fraction", "winning_loss", "winning_kl")}
    pipeline = [
        f"{args.samplers} continuous CPU actor{'s' if args.samplers > 1 else ''} → "
        + (f"{args.segment_steps}-decision bootstrapped segments" if args.segment_steps
           else "complete terminal trajectories"),
        "Bounded queue → policy-lag and action-ratio freshness filters",
        "Uniform reusable rows; prefilter forced/stale/ratio-invalid; priority −3 per use",
        f"{model.layers}-layer card encoder + party/enemy/map summaries + action-object menu → heads",
        f"Asynchronous clipped PPO; full model updated every iteration; "
        f"weights published every {args.publish_updates} updates",
    ]
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
    collect_seconds = update_seconds = screen_seconds = 0.0
    screen_unpack_seconds = screen_forward_seconds = 0.0
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
    threaded = args.sampler_backend == "thread"
    context = multiprocessing.get_context(os.environ.get("SAMPLER_START", "spawn"))
    worker_type = threading.Thread if threaded else context.Process
    queue_type = Queue if threaded else context.Queue
    if threaded:
        os.environ["RAYON_NUM_THREADS"] = str(args.samplers * args.sampler_threads)
    models = [queue_type(maxsize=1) for _ in range(args.samplers)]
    sample_capacity = max(8, args.samplers * 16)
    samples = queue_type(maxsize=sample_capacity)
    results = queue_type(); stop = threading.Event() if threaded else context.Event()
    heartbeat = [started] * args.samplers if threaded else context.Array("d", [started] * args.samplers)
    progress = [0] * args.samplers if threaded else context.Array("q", [0] * args.samplers)
    packer = ThreadPoolExecutor(max_workers=1)
    workers = [None] * args.samplers
    actor = export_value_model(None, model, fingerprint, 1, 0, True)

    def start_worker(worker):
        heartbeat[worker] = time.monotonic()
        progress[worker] = 0
        process = worker_type(target=collect_worker, args=(
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
                    state = export_value_model(None, model, fingerprint, 1, 0, True)
                queue.put_nowait((updates, state))
            except Full:
                pass

    def ingest(item):
        nonlocal decisions, handled, forced, discarded_steps, sampled, collect_seconds, winning_added, orphan_empty_actions, latest_sampler_version, latest_sampler_iteration, dataset_peak, segmented_trajectories, queue_full_waits, queue_put_seconds, queue_delay_sum, queue_packets, queue_peak
        worker, version, result = item
        sampler_versions[worker] = version
        sampler_iterations[worker] = result["iteration"]
        latest_sampler_version = min(sampler_versions)
        latest_sampler_iteration = max(sampler_iterations)
        added, excluded = dataset.add(result, args, progress_active)
        decisions += added
        handled += excluded
        forced += excluded
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
        for character, collected in enumerate(result["episodes"]):
            promotion_episodes.extend((character, row) for row in collected)
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

    def drain_samples():
        for _ in range(sample_capacity):
            try:
                ingest(samples.get_nowait())
            except (Empty, EOFError, OSError):
                break

    def prepare(rows):
        prepared = time.monotonic()
        return unpack(rows, torch.device("cpu"), model, False), time.monotonic() - prepared

    def reserve_batch(size):
        order = dataset.sample(len(dataset), rng)
        selected = order[:size]
        rows = [dataset.rows[index] for index in selected]
        values = {key: dataset.data[key][selected].copy() for key in dataset.data}
        return selected, order, size, rows, values, [], packer.submit(prepare, rows)

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
        nonlocal reported_steps, reported_seconds, reported_trajectories
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
        if target.type == "mps":
            torch.mps.empty_cache()
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
            "collect_seconds": collect_seconds, "screen_seconds": screen_seconds,
            "screen_unpack_seconds": screen_unpack_seconds,
            "screen_forward_seconds": screen_forward_seconds,
            "update_seconds": update_seconds,
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
            "dataset_rows": len(dataset), "dataset_peak": dataset_peak,
            "dataset_seen": dataset.seen, "dataset_admitted": dataset.admitted,
            "dataset_uses": dataset.uses, "dataset_retired": dataset.retired,
            "dataset_forced_dropped": dataset.forced_dropped,
            "dataset_priority_mean": float(dataset.data["priority"].mean()) if len(dataset) else 0,
            "dataset_priority_max": float(dataset.data["priority"].max()) if len(dataset) else 0,
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
            "sample_queue_capacity": sample_capacity, "sample_queue_peak": queue_peak,
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
    promotion_result = None
    def update_promotion():
        nonlocal promotion_ready, promotion_result
        if promotion_ready or stage + 1 >= len(STAGES):
            return
        promotion_ready, promotion_result = promotion_sample(
            promotion_episodes, args.promotion_window, args.promote_win_rate,
        )
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
            failed = not process.is_alive() and (
                threaded or process.exitcode not in (None, 0)
            )
            if not wedged and not failed:
                continue
            if threaded:
                stop.set(); sampler_exhausted[worker] = True
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
            drain_samples()
            restart_stalled()
            sampler_done = (stop.is_set() or all(not worker.is_alive() for worker in workers)) and samples.empty()
            if not pending:
                if len(dataset) < args.batch and not sampler_done:
                    try:
                        ingest(samples.get(timeout=.1))
                        drain_samples()
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
                    update_promotion()
                    if promotion_ready:
                        stop.set()
                    continue
                if not len(dataset):
                    if sampler_done:
                        break
                    continue
                if len(dataset) < args.batch and not sampler_done:
                    continue
            while len(pending) < 1:
                ingested = False
                if not sampler_done:
                    before = decisions
                    drain_samples()
                    ingested = decisions != before
                    sampler_done = (stop.is_set() or all(not worker.is_alive() for worker in workers)) and samples.empty()
                boundary = min(next_report, next_save) - (base_decisions + handled)
                enough = len(dataset) >= args.batch or sampler_done and len(dataset)
                if boundary <= 0:
                    break
                if not enough:
                    if sampler_done or not ingested:
                        break
                    continue
                pending.append(reserve_batch(min(args.batch, len(dataset)) if sampler_done else args.batch))
            if not pending:
                if sampler_done and not len(dataset):
                    break
                continue
            update_started = time.monotonic()
            selected, order, cursor, rows, values, replay, packed = pending.pop(0)
            if not rows:
                continue
            screen_started = time.monotonic()
            unpack_started = time.monotonic()
            cpu_inputs, unpack_elapsed = packed.result()
            screen_unpack_seconds += unpack_elapsed
            inputs = upload(cpu_inputs, target)
            unpack_seconds = time.monotonic() - unpack_started
            unpack_durations.append(unpack_seconds)
            action = torch.as_tensor(values["action"], device=target)
            old = torch.as_tensor(values["old"], device=target)
            lengths = np.asarray([packed_action_count(row) for row in rows], np.int64)
            choice_index = torch.as_tensor(
                np.cumsum(lengths) - lengths + values["action"], device=target,
            )
            flat_policy = not replay
            forward_started = time.monotonic()
            all_logits, all_prediction, all_progress_prediction = predict(
                model, inputs, args.precision, args.policy_temperature, flat_policy=flat_policy,
            )
            forward_seconds = time.monotonic() - forward_started
            screen_forward_seconds += forward_seconds
            screen_seconds += time.monotonic() - screen_started
            forward_durations.append(forward_seconds)
            backward_seconds = 0.0
            prediction = all_prediction[:len(rows)]
            progress_prediction = all_progress_prediction[:len(rows)]
            legal = inputs[6][:len(rows)]
            logits = all_logits[:sum(lengths)] if flat_policy else all_logits[:len(rows)]
            log_ratio = (logits[choice_index] if flat_policy else
                         logits.gather(1, action[:, None]).squeeze(1)) - old
            if not (legal.sum(1) > 1).all():
                raise RuntimeError("forced action entered the dataset")
            fresh = log_ratio.abs() <= args.max_log_ratio
            invalid = ~fresh.detach().cpu().numpy()
            rejected = selected[invalid].tolist()
            if rejected:
                selected = selected[~invalid].tolist()
                target_size = len(rows)
                while len(selected) < target_size and cursor < len(order):
                    candidate = order[cursor:cursor + target_size - len(selected)]
                    cursor += len(candidate)
                    candidate_rows = [dataset.rows[index] for index in candidate]
                    candidate_values = {
                        key: dataset.data[key][candidate].copy() for key in dataset.data
                    }
                    started_screen = time.monotonic()
                    candidate_inputs = upload(prepare(candidate_rows)[0], target)
                    candidate_lengths = np.asarray([
                        packed_action_count(row) for row in candidate_rows
                    ], np.int64)
                    candidate_choice = torch.as_tensor(
                        np.cumsum(candidate_lengths) - candidate_lengths + candidate_values["action"],
                        device=target,
                    )
                    with torch.inference_mode():
                        candidate_logits = predict(
                            model, candidate_inputs, args.precision, args.policy_temperature,
                            policy_only=True, flat_policy=True,
                        )
                    candidate_valid = (
                        candidate_logits[candidate_choice]
                        - torch.as_tensor(candidate_values["old"], device=target)
                    ).abs() <= args.max_log_ratio
                    screen_seconds += time.monotonic() - started_screen
                    candidate_valid = candidate_valid.cpu().numpy()
                    selected.extend(candidate[candidate_valid].tolist())
                    rejected.extend(candidate[~candidate_valid].tolist())
                if not selected:
                    dataset.ratio_dropped += len(rejected)
                    dataset.discard(np.asarray(rejected, np.int64))
                    handled += len(rejected)
                    continue
                selected = np.asarray(selected, np.int64)
                rows = [dataset.rows[index] for index in selected]
                values = {key: dataset.data[key][selected].copy() for key in dataset.data}
                unpack_started = time.monotonic()
                inputs = upload(prepare(rows)[0], target)
                unpack_seconds += time.monotonic() - unpack_started
                action = torch.as_tensor(values["action"], device=target)
                old = torch.as_tensor(values["old"], device=target)
                lengths = np.asarray([packed_action_count(row) for row in rows], np.int64)
                choice_index = torch.as_tensor(
                    np.cumsum(lengths) - lengths + values["action"], device=target,
                )
                forward_started = time.monotonic()
                all_logits, all_prediction, all_progress_prediction = predict(
                    model, inputs, args.precision, args.policy_temperature, flat_policy=flat_policy,
                )
                forward_seconds += time.monotonic() - forward_started
                prediction = all_prediction[:len(rows)]
                progress_prediction = all_progress_prediction[:len(rows)]
                legal = inputs[6][:len(rows)]
                logits = all_logits[:sum(lengths)]
                log_ratio = logits[choice_index] - old
                if not (log_ratio.abs() <= args.max_log_ratio).all():
                    raise RuntimeError("batch eligibility changed after screening")
            selected = np.asarray(selected, np.int64)
            expired = dataset.use(selected)
            removed = np.asarray(rejected + expired.tolist(), np.int64)
            dataset.ratio_dropped += len(rejected)
            if len(removed):
                dataset.discard(np.unique(removed))
            handled += len(removed)
            if not pending and not sampler_done:
                drain_samples()
                boundary = min(next_report, next_save) - (base_decisions + handled)
                if boundary > 0 and len(dataset) >= args.batch:
                    pending.append(reserve_batch(args.batch))
            policy_lags.extend((updates - values["version"]).tolist())
            fresh = torch.ones_like(log_ratio, dtype=torch.bool)
            critic_mask = mask = torch.ones_like(log_ratio)
            critic_count = critic_mask.sum()
            fresh_count = mask.sum()
            critic_denominator = critic_count.clamp_min(1)
            denominator = fresh_count.clamp_min(1)
            safe_log_ratio = torch.where(fresh, log_ratio, torch.zeros_like(log_ratio))
            ratio = safe_log_ratio.exp()
            kl = ((ratio - 1 - safe_log_ratio) * mask).sum().div(denominator).detach()
            fresh_cpu = fresh.detach().cpu().numpy()
            fresh_rows = int(fresh_count)
            attempted += len(rows)
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
            if flat_policy:
                action_row = inputs[5][0][:len(logits)]
                action_legal = inputs[5][2][:len(logits)]
                entropy_by_row = logits.new_zeros(len(rows)).index_add(
                    0, action_row, -(logits.exp() * logits * action_legal),
                )
            else:
                entropy_by_row = -(logits.exp() * logits * legal).sum(1)
            entropy = (entropy_by_row * mask).sum() / denominator
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
                    winning_evicted_characters[packed_character(sample[0])] += 1
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
            accepted, proposals, post_log_ratio = trust_region_step(
                model, optimizer, inputs, action, old, fresh, denominator,
                args.precision, args.target_kl, args.policy_temperature,
                choice_index=choice_index if flat_policy else None,
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
            post_ratio = post_log_ratio.exp()
            observed_kl = post_kl
            observed_clip = float(
                (((post_ratio - 1).abs() > args.clip) * fresh).sum() / denominator
            )
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
            if replay_valid is not None:
                replay_characters = [packed_character(sample[0]) for sample in replay]
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
                represented_actions = max(packed_action_count(row) for row in packed)
                print(json.dumps({"slow_update": {
                    "seconds": update_elapsed,
                    "fresh": len(rows), "replay": len(replay),
                    "state_tokens_max": max(packed_state_count(row) for row in packed),
                    "represented_actions_max": represented_actions,
                    "state_attention_pairs": sum((packed_state_count(row) + 1) ** 2 for row in packed),
                    "legal_actions_max": max(packed_legal_count(row) for row in packed),
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
                update_promotion()
                if promotion_ready:
                    stop.set()
    finally:
        stop.set()
        packer.shutdown(wait=True, cancel_futures=True)
        started_workers = [worker for worker in workers if worker is not None and (threaded or worker.pid is not None)]
        shutdown_deadline = time.monotonic() + 5
        while any(worker.is_alive() for worker in started_workers) and time.monotonic() < shutdown_deadline:
            drain_results()
            for worker in started_workers:
                worker.join(.05)
        terminated = set() if threaded else {worker for worker in started_workers if worker.is_alive()}
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
    failed = [] if threaded else [worker.exitcode for worker in started_workers
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
        "collect_seconds": collect_seconds, "screen_seconds": screen_seconds,
        "screen_unpack_seconds": screen_unpack_seconds,
        "screen_forward_seconds": screen_forward_seconds,
        "update_seconds": update_seconds,
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
        "promotion_result": promotion_result,
        "dataset_rows": len(dataset), "dataset_peak": dataset_peak,
        "dataset_seen": dataset.seen, "dataset_admitted": dataset.admitted,
        "dataset_uses": dataset.uses, "dataset_retired": dataset.retired,
        "dataset_forced_dropped": dataset.forced_dropped,
        "dataset_priority_mean": float(dataset.data["priority"].mean()) if len(dataset) else 0,
        "dataset_priority_max": float(dataset.data["priority"].max()) if len(dataset) else 0,
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
        "sample_queue_capacity": sample_capacity, "sample_queue_peak": queue_peak,
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
                rows = pack_batch(observation, False)
                if len(rows) != count:
                    raise ValueError("packed observation row count mismatch")
                inputs = unpack([rows[index] for index in indices], target, model, False)
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
body{font:14px system-ui;margin:24px;background:#101319;color:#e8ecf2}h1,h2{margin-bottom:6px}.controls{display:flex;align-items:center;gap:12px;flex-wrap:wrap;margin-bottom:22px}select,input{padding:7px;background:#202938;color:#e8ecf2;border:1px solid #526176;border-radius:5px}.charts{display:grid;grid-template-columns:repeat(auto-fit,minmax(440px,1fr));gap:14px}.panel{margin:22px 0;padding:16px;background:#171d28;border:1px solid #303a49;border-radius:10px}.charts .panel{margin:0}.plot{height:340px;min-width:0}table{border-collapse:collapse;width:100%}th,td{padding:7px;border-bottom:1px solid #303a49;text-align:left}.yes{color:#75db91}.no{color:#ee7b7b}
</style><h1 id=title>Spirefysh dashboard</h1><div class=controls><label>Run <select id=version></select></label><label>X axis <select id=xaxis><option value=updates>Optimizer steps</option><option value=decisions selected># decisions</option><option value=time>Wall-clock time</option></select></label><label><input id=smooth type=checkbox checked> EMA</label><label>EMA α <input id=ema type=number min=.01 max=1 step=.01 value=.2></label><span>Auto-refresh 15s</span></div><div class=charts><section class=panel><h2>Mean advantage</h2><div id=advantage class=plot></div></section><section class=panel><h2>Decisions / second</h2><div id=throughput class=plot></div></section><section class=panel><h2>Terminal floor</h2><div id=floor class=plot></div></section><section class=panel><h2>Ascension</h2><div id=ascension class=plot></div></section><section class=panel><h2>Bonus strength</h2><div id=bonus class=plot></div></section><section class=panel><h2>Win proportion</h2><div id=wins class=plot></div></section><section class=panel><h2>Clip fraction</h2><div id=clip class=plot></div></section><section class=panel><h2>KL</h2><div id=kl class=plot></div></section><section class=panel><h2>Entropy</h2><div id=entropy class=plot></div></section></div><section class=panel><h2>Promotion</h2><div id=promotion></div></section><script>const versions=""" + data + r""",versionSelect=document.querySelector('#version'),xaxis=document.querySelector('#xaxis'),smooth=document.querySelector('#smooth'),ema=document.querySelector('#ema');
const characterNames=['Ironclad','Defect','Silent','Regent','Necrobinder'],characterColors=['#ef4444','#38bdf8','#22c55e','#f59e0b','#a78bfa'],names=Object.keys(versions),config={responsive:true,displaylogo:false},refreshKey='spirefysh-dashboard',saved=(()=>{try{return JSON.parse(sessionStorage.getItem(refreshKey))}catch{return null}})();let timeOrigin=0;versionSelect.innerHTML=names.map(name=>`<option value="${name}">V${versions[name].version} · ${name}</option>`).join('');versionSelect.value=saved?.followLatest?names.at(-1):names.includes(saved?.version)?saved.version:names.at(-1)||'';if(saved?.xaxis)xaxis.value=saved.xaxis==='iteration'?'updates':saved.xaxis;if(typeof saved?.smooth==='boolean')smooth.checked=saved.smooth;if(saved?.ema)ema.value=saved.ema;
function updateSteps(history,run){const starts=(run.manifest.sessions||[]).map(session=>Number(session.step)).sort((left,right)=>left-right);let offset=0,last=0,previous=-Infinity,index=0;for(const report of history){let boundary=false;while(starts[index]<report.step){boundary||=starts[index]>previous;index++}const updates=Number(report.metrics.updates)||0;if(boundary||updates<last){offset+=last;last=0}report._updates=offset+updates;last=Math.max(last,updates);previous=report.step}}function x(report){return xaxis.value==='time'?(Number.isFinite(Number(report._written))?(report._written-timeOrigin)/60:(report.metrics.seconds||0)/60):xaxis.value==='decisions'?report.step:report._updates}function series(history,key){return history.map(report=>({x:x(report),y:Number(report.metrics[key])})).filter(point=>Number.isFinite(point.y))}
function emaLine(points){const alpha=Math.max(.01,Math.min(1,Number(ema.value)||.2));let value;return points.map((point,index)=>({x:point.x,y:value=index?alpha*point.y+(1-alpha)*value:point.y}))}
function stageTransitions(history,run){const promotions=(run?.promotions||[]).filter(row=>row.promoted);if(promotions.length)return promotions.map(promotion=>{const report=history.find(row=>row.step===promotion.step)||history.filter(row=>row.step<=promotion.step).at(-1),after=history.find(row=>row.step>promotion.step),next=run.manifest.stages?.[(promotion.stage?.index??-1)+1]||after?.stage,position=xaxis.value==='decisions'?promotion.step:report&&xaxis.value==='updates'?report._updates:Number.isFinite(Number(promotion._written))?(promotion._written-timeOrigin)/60:Number.isFinite(Number(promotion.seconds))?promotion.seconds/60:report?x(report):NaN;return{x:position,stage:next}}).filter(point=>Number.isFinite(Number(point.x)));return history.slice(1).flatMap((row,index)=>Number.isFinite(Number(row.stage?.ascension))&&Number.isFinite(Number(history[index].stage?.ascension))&&(row.stage.ascension!==history[index].stage.ascension||row.stage.bonus!==history[index].stage.bonus)?[{x:x(row),stage:row.stage}]:[])}
function stageLines(history,run){return stageTransitions(history,run).map(point=>({type:'line',xref:'x',yref:'paper',x0:point.x,x1:point.x,y0:0,y1:1,layer:'below',line:{color:'rgba(232,236,242,.38)',width:1,dash:'dash'}}))}
function layout(percent=false,range,history=[],run){const title=xaxis.value==='time'?'Wall-clock time (minutes)':xaxis.value==='decisions'?'# decisions':'Optimizer steps';return{template:'plotly_dark',paper_bgcolor:'rgba(0,0,0,0)',plot_bgcolor:'rgba(0,0,0,0)',margin:{l:62,r:18,t:12,b:52},hovermode:'closest',showlegend:true,shapes:stageLines(history,run),xaxis:{title,gridcolor:'#303a49'},yaxis:{gridcolor:'#303a49',tickformat:percent?'.0%':undefined,range,zerolinecolor:'#8794a8'}}}
function plot(id,points,{range,percent=false,tozero=false,history=[],run}={}){const traces=[{x:points.map(point=>point.x),y:points.map(point=>point.y),mode:'lines+markers',name:'raw',line:{color:'#6fb1ff',width:2},marker:{color:'#6fb1ff',size:6,opacity:.8},hovertemplate:'x %{x}<br>y %{y:.5g}<extra></extra>'}];if(smooth.checked&&points.length>1){const line=emaLine(points);traces.push({x:line.map(point=>point.x),y:line.map(point=>point.y),mode:'lines',name:`EMA α=${Number(ema.value)||.2}`,line:{color:'#ffb454',width:4},hovertemplate:'EMA %{y:.5g}<extra></extra>'})}const options=layout(percent,range,history,run);if(tozero)options.yaxis.rangemode='tozero';Plotly.react(id,traces,options,config)}
function floorPlot(history,run){const training=run.manifest.sessions?.at(-1)?.training||run.manifest.training||{},envs=training.envs||1,points=[];for(const report of history){let inferred=0,last=-Infinity;for(const row of report.metrics.trajectory_floors||[]){const [iteration,floor,stored]=row;if(stored===undefined&&iteration<last)inferred++;const character=stored??Math.min(inferred,4);points.push({x:xaxis.value==='updates'?report._updates:xaxis.value==='decisions'?iteration*envs:(report.metrics.seconds||0)/60,y:floor,character,updates:report._updates,step:report.step});last=iteration}}for(let i=points.length-1;i>0;i--){const j=Math.floor(Math.random()*(i+1));[points[i],points[j]]=[points[j],points[i]]}const trajectory={x:points.map(point=>point.x),y:points.map(point=>point.y),customdata:points.map(point=>[characterNames[point.character],point.updates,point.step]),mode:'markers',showlegend:false,marker:{color:points.map(point=>characterColors[point.character]),size:6,opacity:.5},hovertemplate:'%{customdata[0]}<br>floor %{y}<br>optimizer steps %{customdata[1]:,}<br>report decisions %{customdata[2]:,}<extra></extra>'},legend=characterNames.map((name,character)=>({x:[null],y:[null],mode:'markers',name,hoverinfo:'skip',marker:{color:characterColors[character],size:7,opacity:.5}})),options=layout(false,[0,52],history,run);options.yaxis.title='Terminal floor';Plotly.react('floor',[trajectory,...legend],options,config)}
function stagePlot(id,history,key,color,run){const rows=history.filter(row=>Number.isFinite(Number(row.stage?.[key]))).map(row=>({x:x(row),y:row.stage[key]}));for(const transition of stageTransitions(history,run))if(Number.isFinite(Number(transition.stage?.[key])))rows.push({x:transition.x,y:transition.stage[key]});rows.sort((left,right)=>left.x-right.x);const options=layout(false,undefined,history,run);options.yaxis={...options.yaxis,title:key==='ascension'?'Ascension':'Bonus strength',rangemode:'tozero',dtick:key==='ascension'?1:4};Plotly.react(id,[{x:rows.map(row=>row.x),y:rows.map(row=>row.y),mode:'lines+markers',name:key==='ascension'?'Ascension':'Bonus strength',line:{color,width:3,shape:'hv'},marker:{color,size:6},hovertemplate:`${key==='ascension'?'ascension':'bonus'} %{y}<extra></extra>`}],options,config)}
function promotionSummary(row){if(!row)return '<p>No promotion check yet.</p>';const characters=row.result?.characters||[],rows=characters.map(item=>`<tr><td>${characterNames[item.character]??`Character ${item.character}`}</td><td>${item.wins}/${item.runs}</td><td>${(100*item.wins/item.runs).toFixed(1)}%</td><td>${Number(item.floor_mean).toFixed(2)}</td><td>${item.caps}</td></tr>`).join('');return `<p class="${row.promoted?'yes':'no'}">${row.promoted?'Promoted':'Stayed at current stage'} · threshold ${(100*row.threshold).toFixed(0)}% per character · seed ${row.seed}</p><table><thead><tr><th>Character</th><th>Wins</th><th>Rate</th><th>Mean floor</th><th>Caps</th></tr></thead><tbody>${rows}</tbody></table>`}
function saveDashboardState(){const views={};document.querySelectorAll('.plot').forEach(node=>{const view={};if(node._fullLayout?.xaxis?.autorange===false)view.x=[...node._fullLayout.xaxis.range];if(node._fullLayout?.yaxis?.autorange===false)view.y=[...node._fullLayout.yaxis.range];if(view.x||view.y)views[node.id]=view});try{sessionStorage.setItem(refreshKey,JSON.stringify({version:versionSelect.value,followLatest:versionSelect.value===names.at(-1),xaxis:xaxis.value,smooth:smooth.checked,ema:ema.value,scroll:[scrollX,scrollY],views}))}catch{}}
function restoreDashboardState(){if(saved?.version===versionSelect.value&&saved.xaxis===xaxis.value)for(const [id,view] of Object.entries(saved.views||{})){const update={};if(view.x)update['xaxis.range']=view.x;if(view.y)update['yaxis.range']=view.y;if(Object.keys(update).length)Plotly.relayout(id,update)}if(saved?.scroll)scrollTo(...saved.scroll)}
function showVersion(){const run=versions[versionSelect.value],reports=run.reports;updateSteps(reports,run);const timed=reports.find(row=>Number.isFinite(Number(row._written))&&Number.isFinite(Number(row.metrics.seconds)));timeOrigin=timed?timed._written-timed.metrics.seconds:0;document.querySelector('#title').textContent=`Spirefysh V${run.version} · ${run.run}`;plot('advantage',series(reports,'mean_advantage'),{history:reports,run});plot('throughput',series(reports,'decisions_per_second'),{tozero:true,history:reports,run});floorPlot(reports,run);stagePlot('ascension',reports,'ascension','#fb7185',run);stagePlot('bonus',reports,'bonus','#f59e0b',run);plot('wins',reports.map(row=>({x:x(row),y:row.metrics.wins/Math.max(1,row.metrics.episodes)})),{range:[0,1],percent:true,history:reports,run});plot('clip',series(reports,'clip_fraction'),{range:[0,1],percent:true,history:reports,run});plot('kl',series(reports,'kl'),{tozero:true,history:reports,run});plot('entropy',series(reports,'entropy'),{tozero:true,history:reports,run});document.querySelector('#promotion').innerHTML=promotionSummary(run.promotions.at(-1))}versionSelect.onchange=showVersion;xaxis.onchange=showVersion;smooth.onchange=showVersion;ema.oninput=showVersion;showVersion();setTimeout(restoreDashboardState,100);window.addEventListener('beforeunload',saveDashboardState);setInterval(()=>{saveDashboardState();location.reload()},15000)</script>"""
    content = content.replace(
        "<section class=panel><h2>Entropy</h2><div id=entropy class=plot></div></section></div><section class=panel><h2>Promotion</h2>",
        "<section class=panel><h2>Entropy</h2><div id=entropy class=plot></div></section><section class=panel><h2>Dataset size</h2><div id=dataset class=plot></div></section></div><section class=panel><h2>Promotion</h2>",
    ).replace(
        "plot('entropy',series(reports,'entropy'),{tozero:true,history:reports,run});document.querySelector('#promotion')",
        "plot('entropy',series(reports,'entropy'),{tozero:true,history:reports,run});plot('dataset',series(reports,'dataset_rows'),{tozero:true,history:reports,run});document.querySelector('#promotion')",
    )
    content = content.replace(
        "['Priority',`1+|Awin|+|Aprogress|+terminal/win bonuses`,`Each row gets priority 1 + |policy advantage| + |progress advantage| + 4×terminal + 4×winning-return. Sampling uses the square root of priority.",
        "[manifest.model_version>=66?'Reusable priority':'Priority',manifest.model_version>=66?'initial score, then −3 per use':`1+|Awin|+|Aprogress|+terminal/win bonuses`,manifest.model_version>=66?'Rows remain in the dataset after use, lose three priority points, and retire only below zero. Selection itself is uniform.':'Each row gets priority 1 + |policy advantage| + |progress advantage| + 4×terminal + 4×winning-return. Sampling uses the square root of priority.",
    ).replace(
        "['Token-cost bucket',`one bucket per batch`,`Rows are bucketed by floor(log2(estimated token/attention cost)). A bucket is chosen by total priority to reduce padding and attention waste.",
        "[manifest.model_version>=66?'Eligibility screen':'Token-cost bucket',manifest.model_version>=66?'before optimizer batch':`one bucket per batch`,manifest.model_version>=66?'Forced actions are never admitted. Stale and behavior-ratio-invalid rows are removed before the final batch is built.':'Rows are bucketed by floor(log2(estimated token/attention cost)). A bucket is chosen by total priority to reduce padding and attention waste.",
    ).replace(
        "['Balanced one-pass batch',`up to ${n(batch)} rows`,`Each batch draws evenly across the five characters when possible, without replacement, then deletes those rows from the dataset.",
        "[manifest.model_version>=66?'Reusable random batch':'Balanced one-pass batch',`up to ${n(batch)} usable rows`,manifest.model_version>=66?'Rows are sampled uniformly across all compute costs. Used rows remain until their priority falls below zero.':'Each batch draws evenly across the five characters when possible, without replacement, then deletes those rows from the dataset.",
    )
    floor_start = content.index("function floorPlot(")
    floor_end = content.index("function stagePlot(", floor_start)
    content = content[:floor_start] + r"""function floorPlot(history,run){const completed=history.filter(report=>Number(report.metrics.episodes)>0||(report.metrics.trajectory_floors?.length??0)>0),points=[],traces=[],bands=[['min','max','min–max','.04'],['p01','p99','p1–p99','.06'],['p05','p95','p5–p95','.09'],['p10','p90','p10–p90','.13'],['p25','p75','p25–p75','.20']];for(const [low,high,name,alpha] of bands){const rows=completed.map(report=>({x:x(report),low:report.metrics.floor_bands?.[low],high:report.metrics.floor_bands?.[high]})).filter(row=>Number.isFinite(row.low)&&Number.isFinite(row.high));traces.push({x:rows.map(row=>row.x),y:rows.map(row=>row.low),mode:'lines',showlegend:false,hoverinfo:'skip',line:{width:0}},{x:rows.map(row=>row.x),y:rows.map(row=>row.high),mode:'lines',name,line:{width:0},fill:'tonexty',fillcolor:`rgba(111,177,255,${alpha})`,hovertemplate:`${name}<br>upper %{y:.2f}<extra></extra>`})}for(const report of completed){let inferred=0,last=-Infinity;for(const row of report.metrics.trajectory_floors||[]){const [iteration,floor,stored]=row;if(stored===undefined&&iteration<last)inferred++;const character=stored??Math.min(inferred,4);points.push({x:xaxis.value==='updates'?report._updates:xaxis.value==='decisions'?report.step:(report.metrics.seconds||0)/60,y:floor,character,updates:report._updates,step:report.step});last=iteration}}for(let i=points.length-1;i>0;i--){const j=Math.floor(Math.random()*(i+1));[points[i],points[j]]=[points[j],points[i]]}traces.push({x:points.map(point=>point.x),y:points.map(point=>point.y),customdata:points.map(point=>[characterNames[point.character],point.updates,point.step]),mode:'markers',name:'trajectories',marker:{color:points.map(point=>characterColors[point.character]),size:5,opacity:.28},hovertemplate:'%{customdata[0]}<br>floor %{y}<br>optimizer steps %{customdata[1]:,}<br>report decisions %{customdata[2]:,}<extra></extra>'});const means=completed.map(report=>({x:x(report),y:report.metrics.floor_bands?.mean??report.metrics.floor_mean})).filter(point=>Number.isFinite(point.y));traces.push({x:means.map(point=>point.x),y:means.map(point=>point.y),mode:'lines+markers',name:'mean',line:{color:'#ffb454',width:3},marker:{size:5},hovertemplate:'mean %{y:.2f}<extra></extra>'});if(smooth.checked&&means.length>1){const line=emaLine(means);traces.push({x:line.map(point=>point.x),y:line.map(point=>point.y),mode:'lines',name:`mean EMA α=${Number(ema.value)||.2}`,line:{color:'#f97316',width:2,dash:'dot'},hovertemplate:'mean EMA %{y:.2f}<extra></extra>'})}for(const [character,name] of characterNames.entries())traces.push({x:[null],y:[null],mode:'markers',name,hoverinfo:'skip',marker:{color:characterColors[character],size:7,opacity:.6}});const options=layout(false,[0,52],history,run);options.yaxis.title='Terminal floor';Plotly.react('floor',traces,options,config)}
""" + content[floor_end:]
    target.mkdir(parents=True, exist_ok=True)
    temporary = target / "dashboard.html.tmp"
    temporary.write_text(content)
    temporary.replace(target / "dashboard.html")


def train(args):
    qos = getattr(ctypes.CDLL(None), "pthread_set_qos_class_self_np", None)
    if qos is not None:
        qos(0x21, 0)
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
    if source and source.get("_source_model_version") == MODEL_VERSION and source.get("winning_reservoir"):
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
                reservoir.rows[index] = (*row[:4], np.asarray(
                    policy[:packed_action_count(row[0])], np.float16,
                ))
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
    stage_bests = load_stage_bests(output, development_dir, checkpoints_dir, best)
    for key, record in stage_bests.items():
        champion = champions_dir / f"{int(key):02}.pt"
        if not champion.exists():
            link_checkpoint(output / record["checkpoint"], champion)
        record["artifact"] = str(champion.relative_to(output))
    if stage_bests != best.get("stages", {}):
        best["stages"] = stage_bests
        atomic_json(best_path, best)
    def promote(result=None):
        nonlocal stage, stage_decisions, promotion_index, progress_active, promotion_seconds
        path = promotions_dir / f"{decisions:012}.json"
        if path.exists() or stage + 1 >= len(STAGES):
            raise ValueError("promotion is not available at this checkpoint")
        promotion_started = time.monotonic()
        manual = result is None
        if manual:
            panels = sorted(development_dir.glob("*.json"))
            result = json.loads(panels[-1].read_text())["result"] if panels else {
                "runs": 0, "wins": 0, "characters": [],
            }
        promotion_index += 1
        promotion = {
            "schema": 1, "step": decisions, "seed": None,
            "seconds": elapsed_offset + time.monotonic() - started,
            "stage": {"index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1]},
            "trigger": {
                "mode": "manual" if manual else "last sampled trajectories",
                "trajectories": args.promotion_window,
                "minimum_per_character": 1,
                "win_rate": args.promote_win_rate,
            },
            "threshold": args.promote_win_rate, "promoted": True, "result": result,
        }
        immutable_json(path, promotion)
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
        last_checkpoint = None
        def save_step(step):
            nonlocal last_checkpoint
            checkpoint = checkpoints_dir / f"{step:012}.pt"
            immutable_digest = save(checkpoint, step, False)
            link_checkpoint(checkpoint, latest)
            digest = immutable_digest
            last_checkpoint = step, checkpoint, immutable_digest
            atomic_json(output / "latest.json", {
                "step": step, "stage": stage, "sampler_session": sampler_session,
                "checkpoint": latest.name,
                "immutable": str(checkpoint.relative_to(output)),
                "immutable_sha256": immutable_digest, "sha256": digest,
            })
        def save_report(point, pipeline, window):
            row = {
                "schema": 1, "step": point["steps"], "window": window,
                "sampler_session": sampler_session,
                "stage": {"index": stage, "ascension": STAGES[stage][0], "bonus": STAGES[stage][1]},
                "description": f"Continuous V{MODEL_VERSION} training at A{STAGES[stage][0]}/+{STAGES[stage][1]}.",
                "pipeline": pipeline, "metrics": point,
            }
            immutable_json(reports_dir / f"{point['steps']:012}.json", row)
            atomic_json(output / "live.json", row)
            keep = {last_checkpoint[1]} if last_checkpoint else set()
            keep.update(output / row["checkpoint"] for row in stage_bests.values())
            if best.get("checkpoint"):
                keep.add(output / best["checkpoint"])
            for checkpoint in checkpoints_dir.glob("*.pt"):
                if checkpoint not in keep:
                    checkpoint.unlink()
            print(json.dumps(row), flush=True)
        model.train()
        training = train_stream(
            model, optimizer, args, sampler_session, stage, target, deadline, budget,
            base, auxiliary_decisions, stage_decisions, run_started, reservoir, progress_active,
            save_report, save_step, manifest["fingerprint"],
        )
        if not training["decisions"]:
            break
        decisions += training["decisions"]
        auxiliary_decisions += training["decisions"]
        stage_decisions += training["decisions"]
        training_seconds += training["seconds"]
        if not training["promotion_ready"] or time.monotonic() >= deadline:
            break
        promote(training["promotion_result"])
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
    missing, unexpected = model.load_state_dict(checkpoint["model"], strict=True)
    if missing or unexpected:
        raise ValueError("incompatible model migration")
    checkpoint["_optimizer_compatible"] = migrate_optimizer(
        checkpoint["optimizer"], len(tuple(model.parameters()))
    )
    checkpoint["_source_model_version"] = version
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


def export_value_model(path, model, fingerprint, temperature, bias, actor=False):
    path = Path(path) if path is not None else None
    if path is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
    parts = [
        b"STSVALUE", struct.pack("<IIQ", MODEL_VERSION, FEATURE_VERSION, fingerprint),
        struct.pack("<18I", model.width, model.layers, model.heads, model.feedforward,
                    model.card_zones, len(DOMAIN_SPECS), model.layout["globals"], 2,
                    model.entity_collections, model.head_width,
                    ACTION_FIELDS[0], ACTION_FIELDS[1], ACTION_FIELDS[3],
                    model.semantic.num_embeddings, ACTION_FIELDS[2], ACTION_FIELDS[3],
                    6, model.state_width),
        struct.pack("<64I", *(value for _name, unsigned, signed, semantic, numeric in DOMAIN_SPECS
                              for value in (unsigned, signed, semantic, numeric))),
        struct.pack("<2f", temperature, bias),
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
    add_encoder(model.action_encoder)
    add(model.card_state); add(model.card_count.weight)
    add_transformer(model.card_transformer)
    for module in (model.effect_tuple, model.history_tuple, model.actor_pool):
        add_linear(module)
    add(model.actor_state); add_linear(model.actor_mlp[0]); add_norm(model.actor_mlp[1])
    for module in (model.entity_tuple, model.entity_pool):
        add_linear(module)
    add(model.entity_state); add_norm(model.graph_norm)
    for module in (model.graph_query, model.graph_key_value, model.graph_edge, model.graph_out,
                   model.graph_degree):
        add_linear(module)
    add_norm(model.graph_ff_norm); add_linear(model.graph_ff[0]); add_linear(model.graph_ff[2])
    add(model.continuation_relation.weight); add_linear(model.continuation_tuple)
    add_linear(model.continuation_pool); add_linear(model.continuation_parent)
    add_norm(model.continuation_norm)
    add(model.candidate_scale); add(model.candidate_bias); add_linear(model.candidate_combine)
    add_linear(model.path_adapter); add_linear(model.target_adapter); add(model.action_legal.weight)
    add_norm(model.action_norm)
    for module in (model.menu_object, model.menu_query, model.menu_key_value, model.menu_out,
                   model.menu_count):
        add_linear(module)
    add_norm(model.menu_norm); add(model.menu_empty)
    add_linear(model.value[0]); add_linear(model.value[2])
    if actor:
        flush()
        parts.append(b"STSACTOR")
        for head in (model.policy, model.progress_value):
            add_linear(head[0]); add_linear(head[2])
    flush()
    data = b"".join(parts)
    if path is None:
        return data
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_bytes(data); temporary.replace(path)
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
    base = model.decide(state, action, row, torch.arange(3, device=target), 3, legal, sequence, 3)
    value = model.value(state)
    permutation = torch.tensor((2, 0, 1), device=target)
    permuted = model.decide(
        state, action[permutation], row, permutation, 3, legal, sequence, 3,
    )
    permuted_value = model.value(state)
    assert torch.allclose(base, permuted, atol=1e-6, rtol=1e-6)
    assert torch.allclose(value, permuted_value, atol=1e-6, rtol=1e-6)
    single_sequence = candidate_index(np.zeros(1, np.int32), 1, target)
    single = model.decide(
        state, action[:1], row[:1], row[:1], 1, legal[:1], single_sequence, 1,
    )
    assert single.item() == 0


def probe_action_menu(model, target):
    state = torch.randn(1, model.base_state_width, device=target)
    actions = torch.randn(2, model.width, device=target)
    grouped = (
        torch.zeros(2, dtype=torch.long, device=target), torch.full((1,), 2., device=target),
        torch.zeros(1, dtype=torch.long, device=target),
        candidate_index(np.zeros(1, np.int32), 1, target),
    )
    assert torch.allclose(
        model.encode_menu(state, actions, grouped), model.encode_menu(state, actions.flip(0), grouped),
        atol=1e-6, rtol=1e-6,
    )
    assert not torch.allclose(
        model.encode_menu(state, actions, grouped),
        model.encode_menu(state, torch.stack((actions[0], actions[1] + 1)), grouped),
    )
    single = (
        torch.zeros(1, dtype=torch.long, device=target), torch.ones(1, device=target),
        torch.zeros(1, dtype=torch.long, device=target),
        candidate_index(np.zeros(1, np.int32), 1, target),
    )
    assert torch.allclose(
        model.encode_menu(state, actions[:1], single),
        model.encode_menu(state + torch.randn_like(state), actions[:1], single),
        atol=1e-6, rtol=1e-6,
    )
    separate = (
        torch.arange(2, device=target), torch.ones(2, device=target),
        torch.zeros(1, dtype=torch.long, device=target),
        candidate_index(np.zeros(2, np.int32), 1, target),
    )
    assert not torch.allclose(
        model.encode_menu(state, actions, separate),
        model.encode_menu(state + torch.randn_like(state), actions, separate),
    )
    empty = (
        torch.empty(0, dtype=torch.long, device=target), torch.empty(0, device=target),
        torch.empty(0, dtype=torch.long, device=target), candidate_index([], 0, target, True),
    )
    assert torch.allclose(
        model.encode_menu(state, actions[:0], empty)[0], model.menu_norm(model.menu_empty),
    )


def probe():
    torch.manual_seed(7); np.random.seed(7)
    assert bands([])["mean"] is None
    assert episode_summary([])["floor_mean"] is None
    promotion = [
        (character, (int(index < 9), 1, 0, 0, 0, index))
        for character in range(5) for index in range(40)
    ]
    ready, result = promotion_sample(promotion, 200, .2)
    assert ready and all(row["runs"] == 40 and row["win_rate"] == .225
                         for row in result["characters"])
    promotion[8] = (0, (0, 1, 0, 0, 0, 8))
    assert not promotion_sample(promotion, 200, .2)[0]
    assert not promotion_sample(promotion[:199], 200, .2)[0]
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
            layout["head_width"]) == (64, 2, 4, 128, 5, 11, 0, 1152, 64, 112)
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
    assert model.base_state_width == 1088 and model.state_width == 1152
    assert not hasattr(model, "fusion_transformer")
    probe_candidate_policy(model, target)
    probe_action_menu(model, target)
    observation = env.observe_tokens()
    flat_observation = env.observe_tokens(flat=True)
    assert np.asarray(observation[1]).shape == (8, 0)
    python_rows, rust_rows = pack_batch(observation), pack_batch(flat_observation)
    for rows in (python_rows, rust_rows):
        for packed in rows:
            validate_packed(packed)
    def arrays(value):
        if isinstance(value, (tuple, list)):
            return [array for item in value for array in arrays(item)]
        return [np.asarray(value)]
    assert all(np.array_equal(left, right) for left, right in zip(
        arrays(sts2_sim.unpack_packed_observations(python_rows)),
        arrays(sts2_sim.unpack_packed_observations(list(rust_rows))),
    ))

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
    for field in (3, 4):
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
    dataset = ExperienceDataset()
    assert dataset.add(
        {"trajectories": [segment]}, argparse.Namespace(gae_lambda=1., progress_gamma=1.),
    ) == (2, 0)
    selected = dataset.sample(2, np.random.default_rng(19))
    before = dataset.data["priority"].copy()
    expired = dataset.use(selected)
    assert np.allclose(dataset.data["priority"][selected], before[selected] - 3)
    dataset.discard(expired)
    assert len(dataset) == 2 - len(expired)
    mismatched = copy.deepcopy(segment); mismatched["bootstrap_version"] = 4
    try:
        ExperienceDataset().add(
            {"trajectories": [mismatched]},
            argparse.Namespace(gae_lambda=1., progress_gamma=1.),
        )
        raise AssertionError("accepted mixed-version bootstrap")
    except ValueError:
        pass

    inputs = tensors(observation, target, model)
    object_group, object_count, menu_rows, _menu_sequence = inputs[5][8]
    assert torch.equal(torch.bincount(object_group).float(), object_count)
    assert torch.equal(menu_rows, torch.arange(8))
    card_counts = inputs[3][0][-1]
    assert card_counts.shape[1] == 6
    for count, logarithm in ((0, 1), (2, 3), (4, 5)):
        assert torch.allclose(card_counts[:, logarithm], (card_counts[:, count] * 64).log1p() / 5)
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
        assert state.shape == (8, 1152)
        blocks = state[:, :model.base_state_width].reshape(8, -1, model.width)
        assert torch.allclose(blocks.mean(2), torch.zeros_like(blocks[:, :, 0]), atol=1e-5)
        assert torch.equal(output[0], predict(model, inputs, "fp32", policy_only=True))
        flat_policy = predict(model, inputs, "fp32", policy_only=True, flat_policy=True)
        assert torch.equal(flat_policy, output[0].reshape(-1)[inputs[5][1]])
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
        illegal_inputs = _tensors(illegal, target, model)
        absent_inputs = _tensors(absent, target, model)
        illegal_full = model(*illegal_inputs[:6], return_state=True)
        absent_full = model(*absent_inputs[:6], return_state=True)
        illegal_output, absent_output = illegal_full[:3], absent_full[:3]
        padded_output = predict(model, _tensors(padded, target, model), "fp32")
    assert not torch.allclose(illegal_full[3], absent_full[3])
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
    target_group = None
    for _ in range(256):
        target_observation = target_env.observe_tokens()
        target_u = np.asarray(target_observation[3][0])
        target_represented = np.asarray(target_observation[3][4], bool)
        target_legal = np.asarray(target_observation[3][5], bool)
        target_present = target_represented & (target_u[:, :, 1] != 0) & (
            target_u[:, :, 1] != np.iinfo(np.uint32).max
        )
        for batch_row in range(len(target_u)):
            groups = {}
            for column in np.flatnonzero(target_present[batch_row]):
                groups.setdefault((int(target_u[batch_row, column, 0]), int(target_u[batch_row, column, 14])), []).append(column)
            target_group = next((
                (batch_row, columns) for columns in groups.values()
                if len(columns) > 1 and len(set(target_u[batch_row, columns, 1])) > 1
            ), None)
            if target_group:
                break
        if target_group:
            break
        target_env.step(target_legal.argmax(1).tolist())
    assert target_group is not None
    target_inputs = tensors(target_observation, target, model)
    assert np.array_equal(target_inputs[5][7].numpy() >= 0, target_present[target_represented])
    encoded = model.encode_domains(target_inputs[2])
    _state, nodes, actors, continuations = model.encode_state(
        target_inputs[1], target_inputs[2], encoded, target_inputs[3],
    )
    joined = model.encode_actions(
        encoded, continuations, actors, target_inputs[4], target_inputs[5], nodes,
    )[0]
    detached_index = (*target_inputs[5][:7], torch.full_like(target_inputs[5][7], -1),
                      *target_inputs[5][8:])
    detached = model.encode_actions(
        encoded, continuations, actors, target_inputs[4], detached_index, nodes,
    )[0]
    selected = target_inputs[5][7] >= 0
    assert not torch.allclose(joined[selected], detached[selected])
    if (~selected).any():
        assert torch.allclose(joined[~selected], detached[~selected])
    target_row, columns = target_group
    flat = np.full(target_represented.shape, -1, np.int32)
    flat[target_represented] = np.arange(target_represented.sum())
    leaves = torch.as_tensor(flat[target_row, columns], device=target)
    assert target_inputs[5][8][0][leaves].unique().numel() == 1
    assert not torch.allclose(joined[leaves[0]], joined[leaves[1]])

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
    assert cached.cache_stats["graph_hit"]
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
    assert 800_000 <= parameters <= 1_500_000
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
    run.add_argument("--output", default="target/v63")
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
    run.add_argument("--sampler-backend", choices=("process", "thread"), default="process")
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
    run.add_argument("--promotion-window", type=int, default=200)
    run.add_argument("--promotion-trigger-rate", type=float, default=.2)
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
