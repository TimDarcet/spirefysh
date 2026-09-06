import argparse
import atexit
import copy
import ctypes
import fcntl
import hashlib
import json
import logging
import math
import multiprocessing
import os
import pickle
import re
import struct
import sys
import tempfile
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime
from pathlib import Path
from queue import Empty, Full, Queue

import numpy as np
import torch
from torch import nn

import sts2_sim


FEATURE_VERSION = 56
MODEL_VERSION = 72
CATEGORIES = 83
MAX_PROGRESS = 72
PRECISIONS = ("fp32", "bf16")
WINNING_CAPACITY = 0
CHANGE = "V72: 128-D tokens and a four-layer global transformer."
COLLECTION_POOLING = ("sum", "transformer", "global_tokens")
EFFECT_POOLING = (
    "sum_into_actor", "transformer_into_actor", "sum_token", "transformer_token",
    "global_tokens",
)
CONTINUATION_POOLING = ("sum", "transformer", "gru", "global_tokens")
POSITION_CAPS = {
    "enemy": 32, "power": 64, "orb": 16, "map_floor": 64,
    "deck_origin": 256, "draw_top": 256, "draw_bottom": 256,
    "relic": 256, "wax": 256, "continuation": 256,
    "crystal_row": 11, "crystal_column": 11, "crystal_width": 4, "crystal_height": 4,
}
POOLING_DEFAULTS = {
    "relic": "global_tokens", "deck": "global_tokens", "draw": "global_tokens",
    "exhaust": "global_tokens", "discard": "global_tokens", "hand": "global_tokens",
    "orb": "global_tokens", "potion": "global_tokens",
    "enemy_effect": "global_tokens", "friendly_effect": "global_tokens",
    "continuation": "gru", "phase": "transformer", "generation_pool": "sum",
}
STAGES = [(0, bonus) for bonus in (24, 20, 16, 12, 8, 4, 0)] + [
    (ascension, 0) for ascension in range(1, 11)
]


_EVENT_STREAM = None
_ACCELERATOR_LOCK = None
_GLOG = logging.getLogger()
_GLOG_THREADS = []
_GLOG_INFO_FD = None
_GLOG_ACTIVE = False
_GLOG_PATTERN = re.compile(
    rb"^[DIWEF]\d{4} \d{2}:\d{2}:\d{2}\.\d{6} "
)


class GlogFormatter(logging.Formatter):
    def __init__(self, role):
        super().__init__()
        self.role = role

    def format(self, record):
        severity = {logging.DEBUG: "D", logging.INFO: "I", logging.WARNING: "W",
                    logging.ERROR: "E", logging.CRITICAL: "F"}.get(record.levelno, "I")
        when = datetime.fromtimestamp(record.created)
        source = Path(record.pathname).name
        prefix = (
            f"{severity}{when:%m%d %H:%M:%S}.{when.microsecond:06d} "
            f"{record.process:06d} {getattr(record, 'native_tid', record.thread):08d} "
            f"{self.role:<12} {record.threadName:<16} {source:>20}:{record.lineno:05d}] "
        )
        message = record.getMessage()
        if record.exc_info:
            message += "\n" + self.formatException(record.exc_info)
        return "\n".join(prefix + line for line in message.splitlines() or [""])


class BelowWarning(logging.Filter):
    def filter(self, record):
        return record.levelno < logging.WARNING


def configure_logging(output, role, level="INFO"):
    global _GLOG_ACTIVE, _GLOG_INFO_FD
    if _GLOG_ACTIVE:
        return
    logs = Path(output) / "logs"
    logs.mkdir(parents=True, exist_ok=True)
    info_fd = os.open(logs / f"{role}.INFO", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
    _GLOG_INFO_FD = info_fd
    lock = threading.Lock()
    formatter = GlogFormatter(role)

    def capture(fd, name, severity):
        saved = os.dup(fd)
        read_fd, write_fd = os.pipe()
        os.dup2(write_fd, fd); os.close(write_fd)
        capture_fd = os.open(
            logs / f"{role}.{name}", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644,
        )

        def forward():
            with os.fdopen(read_fd, "rb", buffering=0) as source:
                for raw in iter(source.readline, b""):
                    raw = raw.rstrip(b"\r\n")
                    if _GLOG_PATTERN.match(raw):
                        line = raw + b"\n"
                    else:
                        record = logging.LogRecord(
                            "external", severity, f"<{name}>", 0,
                            raw.decode(errors="replace"), (), None,
                        )
                        record.native_tid = threading.get_native_id()
                        line = (formatter.format(record) + "\n").encode()
                    os.write(saved, line)
                    with lock:
                        os.write(capture_fd, line)
                        os.write(info_fd, line)
            os.close(capture_fd); os.close(saved)

        thread = threading.Thread(target=forward, name=f"{name}-capture", daemon=True)
        thread.start(); _GLOG_THREADS.append(thread)

    sys.stdout.flush(); sys.stderr.flush()
    capture(1, "stdout", logging.INFO)
    capture(2, "stderr", logging.WARNING)
    sys.stdout.reconfigure(line_buffering=True)
    sys.stderr.reconfigure(line_buffering=True)
    factory = logging.getLogRecordFactory()

    def record_factory(*args, **kwargs):
        record = factory(*args, **kwargs)
        record.native_tid = threading.get_native_id()
        return record

    logging.setLogRecordFactory(record_factory)
    _GLOG.handlers.clear()
    _GLOG.setLevel(getattr(logging, level.upper()))
    stdout = logging.StreamHandler(sys.stdout); stdout.addFilter(BelowWarning())
    stderr = logging.StreamHandler(sys.stderr); stderr.setLevel(logging.WARNING)
    for handler in (stdout, stderr):
        handler.setFormatter(formatter); _GLOG.addHandler(handler)
    _GLOG.propagate = False
    logging.captureWarnings(True)
    sts2_sim.configure_logging(role, level)
    _GLOG_ACTIVE = True
    _GLOG.info("logging_started info=%s stdout=%s stderr=%s",
               logs / f"{role}.INFO", logs / f"{role}.stdout", logs / f"{role}.stderr")


def shutdown_logging():
    global _GLOG_ACTIVE
    if not _GLOG_ACTIVE:
        return
    sys.stdout.flush(); sys.stderr.flush(); logging.shutdown()
    for fd in (1, 2):
        try:
            os.close(fd)
        except OSError:
            pass
    for thread in _GLOG_THREADS:
        thread.join(2)
    if _GLOG_INFO_FD is not None:
        os.close(_GLOG_INFO_FD)
    _GLOG_ACTIVE = False


atexit.register(shutdown_logging)


def emit_event(value, level=logging.INFO):
    line = json.dumps(value, separators=(",", ":"))
    if _EVENT_STREAM is None:
        print(line, flush=True)
    else:
        print(line, file=_EVENT_STREAM, flush=True)
    fields = {key: item for key, item in value.items()
              if key not in ("event", "metrics", "pipeline", "result")}
    if "metrics" in value:
        fields.update({key: item for key, item in value["metrics"].items()
                       if isinstance(item, (bool, int, float, str))})
    _GLOG.log(level, "%s %s", value.get("event", "event"),
              " ".join(f"{key}={json.dumps(item, separators=(',', ':'))}"
                       for key, item in fields.items()), stacklevel=2)

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


TOKEN_SPECS = (
    ("run", 24, 16, 6, 36), ("phase", 16, 8, 16, 16), ("card", 20, 16, 73, 23),
    ("actor", 12, 16, 5, 27), ("power", 8, 4, 5, 8), ("history", 6, 29, 4, 32),
    ("status", 20, 8, 72, 17), ("relic", 12, 4, 8, 16), ("potion", 6, 2, 5, 6),
    ("orb", 6, 2, 5, 6), ("event", 4, 1, 4, 4), ("encounter", 4, 0, 3, 4),
    ("crystal", 8, 2, 6, 10), ("continuation", 24, 16, 80, 24),
    ("map_node", 12, 0, 7, 8), ("map_edge", 8, 0, 2, 4),
)
DOMAIN = {name: index for index, (name, *_shape) in enumerate(TOKEN_SPECS)}
ACTION_FIELDS = (15, 12, 8, 20)
SEMANTIC_NAMES = (
    "character", "card", "power", "relic", "potion", "enemy", "enemy_move", "orb", "orb_timing",
    "event", "encounter_normal", "encounter_elite", "encounter_boss", "enchantment",
    "card_type", "rarity", "room", "target", "pile", "phase", "action", "effect",
    "run_effect", "condition", "filter", "card_op", "scale",
    "enemy_position", "power_position", "orb_position", "map_floor_position", "deck_origin",
    "draw_top_position", "draw_bottom_position", "relic_position", "wax_position",
    "continuation_position", "crystal_row", "crystal_column", "crystal_width", "crystal_height",
    "flag_bit", "turn_flag_bit", "tag_bit", "run_kind", "phase_kind", "card_zone",
    "card_role", "actor_kind", "power_source", "history_kind", "status_kind",
    "relic_kind", "potion_kind", "orb_kind", "event_kind", "encounter_kind", "crystal_kind",
    "continuation_kind", "continuation_variant", "map_node_kind", "map_edge_kind",
    "route_kind", "requirement", "tinker_rider", "conveyor_dish", "boolean",
    "token_role", "collection",
)


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
                 pooling=None):
        nn.Module.__init__(self)
        self.layout = dict(layout)
        if self.layout["version"] != FEATURE_VERSION:
            raise ValueError("incompatible observation layout")
        expected = {name + suffix: value for name, unsigned, signed, semantic, numeric in TOKEN_SPECS
                    for suffix, value in (("_u", unsigned), ("_s", signed),
                                          ("_c", semantic), ("_f", numeric))}
        expected |= {"domain_count": len(TOKEN_SPECS), "action_u": ACTION_FIELDS[0],
                     "action_s": ACTION_FIELDS[1], "action_c": ACTION_FIELDS[2],
                     "action_f": ACTION_FIELDS[3], "globals": 0, "model_width": 128,
                     "model_layers": 4, "model_heads": 8, "model_feedforward": 384,
                     "action_width": 128}
        if any(self.layout.get(key) != value for key, value in expected.items()):
            raise ValueError("incompatible observation schema")
        if (width, layers, heads, feedforward) != (128, 4, 8, 384) \
                or head_width not in (None, 128):
            raise ValueError("invalid architecture")
        self.width, self.layers, self.heads, self.feedforward = width, layers, heads, feedforward
        self.head_width = width
        self.state_width = width
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
        self.critic = nn.Linear(width, CATEGORIES)
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
        selected = selected[order] if selected is not None else None
        counts = torch.bincount(group, minlength=groups)
        if values.device.type == "mps" and self.width // self.heads in (16, 32):
            lengths = counts + 1
            padded_groups = (groups // 256 + 1) * 256
            lengths = nn.functional.pad(lengths, (0, padded_groups - groups), value=1)
            total = len(values) + padded_groups
            extra = (-total) % (65_536 if groups >= 4096 else 4096)
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
                    positions = torch.cat((offsets[:-1], destination[selected])).sort().values
                    query_counts = torch.bincount(sequence[positions], minlength=len(lengths))
                    query_offsets = torch.cat((query_counts.new_zeros(1), query_counts.cumsum(0)))
                    query_position = torch.arange(len(positions), device=values.device) \
                        - torch.repeat_interleave(query_offsets[:-1], query_counts)
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
                    items = current[query_position > 0][torch.argsort(order[selected])]
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
        selected = torch.arange(len(values), device=values.device) >= len(values) - len(action)
        state, transformed = self._sequence(
            values, rows, batch, self.concepts.local("token_role", 0), self.global_transformer,
            selected,
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
        "value_heads": [f"terminal_progress_categorical_{CATEGORIES}"],
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


def critic_probabilities(logits):
    return logits.softmax(-1)


def critic_expected(probabilities):
    categories = torch.arange(CATEGORIES, device=probabilities.device, dtype=probabilities.dtype)
    return probabilities @ categories / (CATEGORIES - 1)


def critic_win_logit(logits):
    return logits[..., -1] - logits[..., :-1].logsumexp(-1)


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
    parameters = parameters or tuple(model.critic.parameters())
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
        policies = policies if policies is not None else [None] * len(rows)
        for index, (row, choice, log_probability, policy, value) in enumerate(
            zip(rows, choices, log_probabilities, policies, values)
        ):
            if packed_legal_count(row) <= 1:
                self.forced += 1
                continue
            episode = self.pending[index]
            self.pending_seen[index] += 1
            item = (row, int(choice), float(log_probability), 1.0 - float(value),
                    None if policy is None else
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
        groups = self.by_character()
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

    def by_character(self):
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
        logits, critic_logits = predict(
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
            policy, critic_probabilities(critic_logits).cpu().numpy())


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
    fixed_entropy = getattr(args, "entropy_weight", None)
    beta = 0 if not active else (
        args.progress_beta if args.progress_beta is not None
        else 1 - .9 * progress_fraction
    )
    entropy_start, entropy_end = (
        (fixed_entropy, fixed_entropy) if fixed_entropy is not None
        else (args.entropy_start * scale, args.entropy_end * scale)
    )
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


def bounded_mcts_timeout(requested, watchdog):
    return min(requested or watchdog / 2, watchdog / 2)


def sampler_wedged(heartbeat, observed_stale, now, timeout):
    return (now - heartbeat > timeout and observed_stale is not None
            and now - observed_stale > timeout / 10)


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

    def event(self, event, **values):
        _GLOG.info("%s %s", event, " ".join(
            f"{key}={json.dumps(value, separators=(',', ':'))}" for key, value in values.items()
        ), stacklevel=2)

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
            "rows", "choices", "old_log", "critic_probabilities", "canonical_progress",
            "phases", "win_rewards", "terminals", "characters", "versions",
        )
        def materialize(trajectory):
            samples = trajectory.pop("samples")
            trajectory.update(zip(sample_keys, map(list, zip(*samples))))
            return trajectory
        discarded_steps = orphan_empty_actions = sampled_steps = 0
        search_stats = np.zeros(17, np.int64)
        expert_rows = []
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
                self.observation = self.env.observe_tokens(flat=True)
            step_started = time.monotonic()
            state_stats = self.env.stats()
            canonical = np.asarray([row[9] for row in state_stats], np.uint8)
            phases = np.asarray([row[4] for row in state_stats], np.uint8)
            if native:
                (characters, choice, log_probability, critic_probability, step_rows,
                 step_experts, step_search_stats) = self.env.policy(
                    args.policy_temperature,
                    mcts_fraction=args.mcts_fraction,
                    mcts_simulations=args.mcts_simulations,
                    mcts_boss_simulations=args.mcts_boss_simulations,
                    mcts_turns=args.mcts_turns,
                    mcts_max_depth=args.mcts_max_depth,
                    mcts_batch_size=args.mcts_batch_size,
                    mcts_min_visits=args.mcts_min_visits,
                    mcts_max_targets=args.mcts_max_targets,
                    mcts_prior_temperature=args.mcts_prior_temperature,
                    mcts_q_temperature=args.mcts_q_temperature,
                    mcts_exploration=args.mcts_exploration,
                    mcts_value_consistency=getattr(args, "search_consistency_weight", 0) > 0,
                    mcts_heuristic=args.mcts_heuristic,
                    mcts_timeout=bounded_mcts_timeout(args.mcts_timeout, args.sampler_timeout),
                )
                if heartbeat is not None:
                    heartbeat[self.worker] = time.monotonic()
                characters = np.asarray(characters, np.uint8)
                critic_probability = np.asarray(critic_probability, np.float16)
                search_stats += np.asarray(step_search_stats, np.int64)
                for expert in step_experts:
                    row, target, visits, depth, *consistency = expert
                    consistency = (((), np.empty(0, np.float32), 1., 0.)
                                   if not consistency else
                                   (tuple(consistency[0]), np.asarray(consistency[1], np.float32),
                                    consistency[2], consistency[3]))
                    expert_rows.append((row, np.asarray(target, np.float16), version,
                                        visits, depth, consistency))
                policy = None
            else:
                characters = np.asarray(self.observation[0], np.uint8)
                choice, log_probability, policy, critic_probability = act(
                    model, self.observation, target, True, precision, self.torch_rng,
                    args.policy_temperature,
                )
                step_rows = _pack_batch(self.observation)
            if not native:
                for index, action in enumerate(choice):
                    self.action_history[index].append(int(action))
            self.reservoir.record(
                step_rows, choice, log_probability, policy, critic_probability[:, -1], self.rng,
            )
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
            if native:
                self.native_steps.append((
                    step_rows, choice, log_probability, critic_probability, canonical, phases,
                    raw_reward, done, characters, version,
                ))
            else:
                for index, row in enumerate(step_rows):
                    trajectory = self.trajectories[index]
                    if trajectory is None:
                        trajectory = {"samples": [], "started": step_started}
                        self.trajectories[index] = trajectory
                    trajectory["samples"].append((
                        row, choice[index], log_probability[index], critic_probability[index],
                        canonical[index], phases[index], raw_reward[index], done[index],
                        characters[index], version,
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
                            key: [step[column] if column == 9 else step[column][index]
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
                raise ValueError("categorical critic requires complete terminal trajectories")
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
            "expert_rows": expert_rows,
            "mcts_roots": int(search_stats[0]), "mcts_simulations": int(search_stats[1]),
            "mcts_leaves": int(search_stats[2]), "mcts_nodes": int(search_stats[3]),
            "mcts_batches": int(search_stats[4]), "mcts_targets": int(search_stats[5]),
            "mcts_turn_starts": int(search_stats[6]),
            "mcts_seconds": float(search_stats[7]) / 1e6,
            "mcts_simulate_seconds": float(search_stats[8]) / 1e6,
            "mcts_encode_seconds": float(search_stats[9]) / 1e6,
            "mcts_inference_seconds": float(search_stats[10]) / 1e6,
            "mcts_backup_seconds": float(search_stats[11]) / 1e6,
            "mcts_rollout_steps": int(search_stats[12]),
            "mcts_rollout_completed": int(search_stats[13]),
            "mcts_rollout_invalid": int(search_stats[14]),
            "mcts_rollout_seconds": float(search_stats[15]) / 1e6,
            "mcts_timeouts": int(search_stats[16]),
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
                "expert_rows": [],
                "mcts_roots": 0, "mcts_simulations": 0, "mcts_leaves": 0,
                "mcts_nodes": 0, "mcts_batches": 0, "mcts_targets": 0,
                "mcts_turn_starts": 0,
                "mcts_seconds": 0.0,
                "mcts_simulate_seconds": 0.0, "mcts_encode_seconds": 0.0,
                "mcts_inference_seconds": 0.0, "mcts_backup_seconds": 0.0,
                "mcts_rollout_steps": 0, "mcts_rollout_completed": 0,
                "mcts_rollout_invalid": 0, "mcts_rollout_seconds": 0.0,
                "mcts_timeouts": 0,
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
                self.event("model", version=version)
            steps = sampling_steps(produced, budget, self.args.envs, self.args.sampler_steps)
            if not steps:
                break
            result = self.collect(
                model, target, precision, deadline, steps, version, stop,
                heartbeat, progress,
            )
            self.event(
                "collect", version=version, iteration=result["iteration"],
                sampled=result["sampled_steps"], trajectories=len(result["trajectories"]),
                rows=sum(len(row["rows"]) for row in result["trajectories"]),
                seconds=round(result["collect_seconds"], 3),
                mcts_roots=result["mcts_roots"], mcts_simulations=result["mcts_simulations"],
                mcts_targets=result["mcts_targets"], mcts_seconds=round(result["mcts_seconds"], 3),
            )
            pending["trajectories"].extend(result["trajectories"])
            pending["expert_rows"].extend(result["expert_rows"])
            for target_episodes, rows in zip(pending["episodes"], result["episodes"]):
                target_episodes.extend(rows)
            for key in (
                "orphan_empty_actions", "collect_seconds",
                "discarded_steps", "sampled_steps", "card_hit", "card_miss",
                "graph_hit", "graph_miss", "mcts_roots", "mcts_simulations",
                "mcts_leaves", "mcts_nodes", "mcts_batches", "mcts_targets",
                "mcts_turn_starts", "mcts_seconds",
                "mcts_simulate_seconds", "mcts_encode_seconds",
                "mcts_inference_seconds", "mcts_backup_seconds",
                "mcts_rollout_steps", "mcts_rollout_completed", "mcts_rollout_invalid",
                "mcts_rollout_seconds", "mcts_timeouts",
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
                    samples.put_nowait((worker, self.generation, version, pending))
                    self.event(
                        "packet", version=version, iteration=pending["iteration"],
                        trajectories=len(pending["trajectories"]), rows=pending_rows,
                        sampled=pending["sampled_steps"], discarded=pending["discarded_steps"],
                    )
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
        return worker, self.generation, version, pending


def collect_worker(model, args, sampler_session, stage, capacity, pending_capacity, iteration,
                   worker, generation, version, models, samples, stop, deadline, budget, results,
                   heartbeat, progress):
    global _ACCELERATOR_LOCK
    if args.sampler_backend == "process" and _ACCELERATOR_LOCK is not None:
        _ACCELERATOR_LOCK.close()
        _ACCELERATOR_LOCK = None
    if args.sampler_backend == "process":
        configure_logging(args.output, f"sampler-{worker}", getattr(args, "log_level", "INFO"))
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
    collector.event(
        "start", pid=os.getpid(), worker=worker, generation=generation,
        session=sampler_session, stage=stage, envs=args.envs, threads=args.sampler_threads,
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
    try:
        result = collector.run(
            model, models, samples, stop, deadline, budget, worker, heartbeat, progress, version,
            target, precision,
        )
        collector.event(
            "stop", version=result[2], iteration=result[3]["iteration"],
            sampled=result[3]["sampled_steps"], discarded=result[3]["discarded_steps"],
        )
        results.put(result)
    except BaseException:
        collector.event("error", version=version)
        _GLOG.exception("sampler_failed version=%d", version)
        raise
    finally:
        if args.sampler_backend == "process":
            shutdown_logging()


class ExperienceDataset:
    def __init__(self):
        self.rows = []
        self.data = {
            "action": np.empty(0, np.int64), "old": np.empty(0, np.float32),
            "advantage": np.empty(0, np.float32),
            "critic_target": np.empty((0, CATEGORIES), np.float16),
            "value": np.empty(0, np.float32), "canonical": np.empty(0, np.int8),
            "phase": np.empty(0, np.int8), "character": np.empty(0, np.int8),
            "priority": np.empty(0, np.float32),
            "version": np.empty(0, np.int64), "id": np.empty(0, np.int64),
        }
        self.next_id = 0
        self.seen = self.admitted = self.uses = self.retired = self.forced_dropped = 0
        self.stale_dropped = self.ratio_dropped = self.kl_dropped = self.post_kl_dropped = 0

    def __len__(self):
        return len(self.rows)

    def add(self, result, args, limit=None):
        trajectories = result["trajectories"]
        if not trajectories:
            return 0, 0, 0
        fields = (
            "rows", "choices", "old_log", "critic_probabilities", "canonical_progress",
            "phases", "win_rewards", "terminals", "characters", "versions",
        )
        rows = [item for trajectory in trajectories for item in trajectory["rows"]]
        advantage = np.empty(len(rows), np.float32)
        targets = np.empty((len(rows), CATEGORIES), np.float16)
        values = np.empty(len(rows), np.float32)
        end = 0
        for trajectory in trajectories:
            length = len(trajectory["rows"])
            if not length or any(len(trajectory[key]) != length for key in fields):
                raise ValueError("invalid trajectory fields")
            start, end = end, end + length
            terminal = np.asarray(trajectory["terminals"], bool)
            probabilities = np.asarray(trajectory["critic_probabilities"], np.float32)
            canonical = np.asarray(trajectory["canonical_progress"], np.int64)
            if (terminal[:-1].any() or not terminal[-1] or probabilities.shape != (length, CATEGORIES)
                    or not np.isfinite(probabilities).all()
                    or not np.allclose(probabilities.sum(1), 1, atol=2e-3)
                    or canonical.min() < 0 or canonical.max() > MAX_PROGRESS):
                raise ValueError("invalid trajectory terminal")
            category = CATEGORIES - 1 if trajectory["win_rewards"][-1] > .5 else int(canonical.max())
            target = np.zeros(CATEGORIES, np.float32); target[category] = 1
            targets[end - 1] = target
            for index in range(length - 2, -1, -1):
                target = (1 - args.critic_lambda) * probabilities[index + 1] \
                    + args.critic_lambda * target
                targets[start + index] = target
            expected = probabilities @ (np.arange(CATEGORIES, dtype=np.float32) / (CATEGORIES - 1))
            values[start:end] = expected
            advantage[start:end] = category / (CATEGORIES - 1) - expected
        terminal = np.asarray([
            item for trajectory in trajectories for item in trajectory["terminals"]
        ], bool)
        wins = targets[:, -1] > .5
        priority = 1 + np.abs(advantage) + 4 * terminal + 4 * wins
        actionable = np.ones(len(rows), bool) if getattr(args, "critic_only", False) else np.asarray([
            packed_legal_count(row) > 1 for row in rows
        ])
        data = {
            "action": np.asarray([
                item for trajectory in trajectories for item in trajectory["choices"]
            ], np.int64),
            "old": np.asarray([
                item for trajectory in trajectories for item in trajectory["old_log"]
            ], np.float32),
            "advantage": advantage, "critic_target": targets, "value": values,
            "canonical": np.asarray([
                item for trajectory in trajectories for item in trajectory["canonical_progress"]
            ], np.int8),
            "phase": np.asarray([
                item for trajectory in trajectories for item in trajectory["phases"]
            ], np.int8),
            "character": np.asarray([
                item for trajectory in trajectories for item in trajectory["characters"]
            ], np.int8),
            "priority": priority.astype(np.float32),
            "version": np.asarray([
                item for trajectory in trajectories for item in trajectory["versions"]
            ], np.int64),
            "id": np.arange(self.next_id, self.next_id + len(rows), dtype=np.int64),
        }
        accepted = len(rows) if limit is None else max(0, min(len(rows), limit))
        rows, actionable = rows[:accepted], actionable[:accepted]
        self.next_id += accepted
        self.rows.extend(row for row, keep in zip(rows, actionable) if keep)
        for key, value in data.items():
            self.data[key] = np.concatenate((self.data[key], value[:accepted][actionable]))
        self.seen += accepted
        self.admitted += int(actionable.sum())
        forced = accepted - int(actionable.sum())
        self.forced_dropped += forced
        return accepted, forced, len(data["action"]) - accepted

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

    def sample(self, size, rng, balanced=False):
        size = min(size, len(self))
        if balanced:
            pools = [list(rng.permutation(np.flatnonzero(self.data["character"] == character)))
                     for character in range(5)]
            selected = []
            while len(selected) < size and any(pools):
                for pool in pools:
                    if pool and len(selected) < size:
                        selected.append(pool.pop())
            return np.asarray(selected, np.int64)
        return rng.choice(len(self), size, replace=False)

    def use(self, indices, decay=3):
        self.uses += len(indices)
        self.data["priority"][indices] -= decay
        expired = indices[self.data["priority"][indices] < 0]
        self.retired += len(expired)
        return expired


class CriticBalance:
    def __init__(self, decay=.99, state=None):
        self.decay = decay
        self.frequencies = [np.zeros(size, np.float64) for size in (5, 14, MAX_PROGRESS + 1)]
        self.initialized = False
        if state:
            self.initialized = bool(state.get("initialized"))
            self.frequencies = [np.asarray(row, np.float64) for row in state["frequencies"]]
        self.reset_report()

    def state_dict(self):
        return {"initialized": self.initialized,
                "frequencies": [row.tolist() for row in self.frequencies]}

    def weights(self, character, phase, floor):
        columns = tuple(np.asarray(column, np.int64) for column in (character, phase, floor))
        batch = [np.bincount(column, minlength=len(frequency)) / len(column)
                 for column, frequency in zip(columns, self.frequencies)]
        if self.initialized:
            for frequency, current in zip(self.frequencies, batch):
                frequency *= self.decay; frequency += (1 - self.decay) * current
        else:
            for frequency, current in zip(self.frequencies, batch):
                frequency[:] = current
            self.initialized = True
        weight = np.prod([
            np.maximum(frequency[column], 1e-8) ** -.5
            for frequency, column in zip(self.frequencies, columns)
        ], axis=0) ** (1 / 3)
        weight = np.clip(weight, .25, 4); weight /= weight.mean()
        for count, column in zip(self.counts, columns):
            count += np.bincount(column, minlength=len(count))
        self.ess += weight.sum() ** 2 / np.square(weight).sum()
        self.rows += len(weight); self.batches += 1
        return weight.astype(np.float32)

    def record_loss(self, target, weighted_category_loss):
        self.target_counts += target.sum(0)
        self.loss_mass += weighted_category_loss.sum(0)

    def reset_report(self):
        self.counts = [np.zeros(size, np.int64) for size in (5, 14, MAX_PROGRESS + 1)]
        self.target_counts = np.zeros(CATEGORIES, np.float64)
        self.loss_mass = np.zeros(CATEGORIES, np.float64)
        self.ess = 0.; self.rows = self.batches = 0

    def report(self):
        result = {
            "critic_preweight_character_counts": self.counts[0].tolist(),
            "critic_preweight_phase_counts": self.counts[1].tolist(),
            "critic_preweight_floor_counts": self.counts[2].tolist(),
            "critic_preweight_target_counts": self.target_counts.tolist(),
            "critic_postweight_loss_mass": self.loss_mass.tolist(),
            "critic_weight_ess": self.ess / max(1, self.batches),
            "critic_weight_ess_fraction": self.ess / max(1, self.rows),
        }
        self.reset_report()
        return result

class ExpertDataset:
    def __init__(self, capacity):
        self.capacity = capacity
        self.rows = []
        self.targets = []
        self.consistencies = []
        self.versions = np.empty(0, np.int64)
        self.visits = np.empty(0, np.int32)
        self.depths = np.empty(0, np.int32)
        self.ids = np.empty(0, np.int64)
        self.next_id = 0
        self.seen = self.used = self.stale = self.evicted = 0

    def __len__(self):
        return len(self.rows)

    def add(self, rows):
        if not rows:
            return
        for row, target, _version, _visits, _depth, consistency in rows:
            target = np.asarray(target, np.float16)
            if (target.shape != (packed_action_count(row),) or not np.isfinite(target).all()
                    or abs(float(target.sum()) - 1) > 2e-3 or (target < 0).any()):
                raise ValueError("invalid expert target")
            children, weights, self_weight, terminal_value = consistency
            if (len(children) != len(weights) or not np.isfinite(weights).all()
                    or (weights < 0).any() or not 0 <= self_weight <= 1
                    or not np.isfinite(terminal_value)
                    or float(weights.sum()) + self_weight > 1.002
                    or not 0 <= terminal_value <= 1 - self_weight - float(weights.sum()) + .002):
                raise ValueError("invalid search consistency target")
        self.rows.extend(row for row, *_ in rows)
        self.targets.extend(np.asarray(target, np.float16) for _, target, *_ in rows)
        self.consistencies.extend(row[5] for row in rows)
        self.versions = np.r_[self.versions, np.asarray([row[2] for row in rows], np.int64)]
        self.visits = np.r_[self.visits, np.asarray([row[3] for row in rows], np.int32)]
        self.depths = np.r_[self.depths, np.asarray([row[4] for row in rows], np.int32)]
        self.ids = np.r_[self.ids, np.arange(self.next_id, self.next_id + len(rows))]
        self.next_id += len(rows)
        self.seen += len(rows)
        if len(self) > self.capacity:
            count = len(self) - self.capacity
            self.evicted += count
            self.discard(np.arange(count))

    def sample(self, size, rng):
        return rng.choice(len(self), min(size, len(self)), replace=False)

    def prune(self, version, lag):
        stale = np.flatnonzero(self.versions < version - lag)
        self.stale += len(stale)
        self.discard(stale)

    def discard(self, indices):
        if not len(indices):
            return
        keep = np.ones(len(self), bool); keep[indices] = False
        self.rows = [row for row, selected in zip(self.rows, keep) if selected]
        self.targets = [row for row, selected in zip(self.targets, keep) if selected]
        self.consistencies = [row for row, selected in zip(self.consistencies, keep) if selected]
        self.versions = self.versions[keep]
        self.visits = self.visits[keep]
        self.depths = self.depths[keep]
        self.ids = self.ids[keep]

    def discard_ids(self, ids):
        self.discard(np.flatnonzero(np.isin(self.ids, ids)))


def train_stream(model, optimizer, args, sampler_session, stage, target, deadline, budget,
                 base_decisions, auxiliary_decisions, stage_decisions, run_started, reservoir, progress_active,
                 save_report, save_step, fingerprint, critic_balance):
    ascension, bonus = STAGES[stage]
    collector_args = copy.copy(args)
    pending_capacity = max(1, reservoir.capacity // args.envs)
    collector_args.envs //= args.samplers
    dataset = ExperienceDataset()
    expert_dataset = ExpertDataset(args.expert_capacity)
    episodes = [[] for _ in range(5)]
    promotion_episodes = []
    losses = {key: [] for key in (
        "mean_advantage", "policy_loss", "expert_loss", "expert_entropy", "expert_kl",
        "expert_rows", "ppo_policy_head_grad_norm", "expert_policy_head_grad_norm",
        "expert_ppo_grad_ratio", "expert_ppo_grad_cosine", "critic_loss",
        "search_consistency_loss", "critic_expected", "critic_win_probability", "entropy", "entropy_weight", "kl",
        "post_kl", "clip_fraction", "winning_loss", "winning_kl",
    )}
    pipeline = [
        f"{args.samplers} continuous CPU actor{'s' if args.samplers > 1 else ''} → "
        + (f"{args.segment_steps}-decision bootstrapped segments" if args.segment_steps
           else "complete terminal trajectories"),
        "Bounded queue → policy-lag and action-ratio freshness filters",
        f"{'Character-balanced' if args.character_balanced else 'Uniform'} reusable rows; "
        f"prefilter forced/stale/ratio-invalid; priority −{args.priority_decay:g} per use",
        f"{model.layers}-layer global Transformer over state, entity, and action tokens → heads",
        f"{CATEGORIES}-class terminal-progress critic; detached backward λ={args.critic_lambda:g} targets",
        "Critic loss balanced by EMA character/phase/canonical-floor frequency",
        "Turn-start native MCTS → expectimax-Q targets and policy-expectation critic transitions",
        ("Frozen encoder and policy; critic head only" if args.critic_only else
         "Asynchronous clipped PPO; full model updated every iteration") + "; "
        f"policy-head LR ×{args.head_learning_rate_multiplier:g}; "
        f"critic LR ×{args.critic_learning_rate_multiplier:g}; "
        f"weights published every {args.publish_updates} updates",
    ]
    winning_behavior_drift = 0
    decisions = discarded_steps = handled = sampled = attempted = forced = trained = updates = windows = 0
    mcts_roots = mcts_simulations = mcts_leaves = mcts_nodes = mcts_batches = mcts_targets = 0
    mcts_turn_starts = 0
    mcts_seconds = mcts_simulate_seconds = mcts_encode_seconds = 0.0
    mcts_inference_seconds = mcts_backup_seconds = mcts_rollout_seconds = 0.0
    mcts_rollout_steps = mcts_rollout_completed = mcts_rollout_invalid = mcts_timeouts = 0
    expert_visits = expert_depth = 0
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
    worker_resolved = [0] * args.samplers
    sampler_generations = [0] * args.samplers
    sampler_restarts = [0] * args.samplers
    sampler_wedges = [0] * args.samplers
    sampler_restart_streaks = [0] * args.samplers
    sampler_recovery_packets = [0] * args.samplers
    sampler_stale_since = [None] * args.samplers
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
    next_heartbeat_log = started
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
    watchdog_terminated = set()
    watchdog_pending = [False] * args.samplers
    watchdog_failures = Queue()
    actor = export_value_model(None, model, fingerprint, 1, 0, True)
    actor_version = updates

    def start_worker(worker):
        heartbeat[worker] = time.monotonic()
        progress[worker] = 0
        sampler_recovery_packets[worker] = 0
        sampler_stale_since[worker] = None
        process = worker_type(target=collect_worker, args=(
            actor, collector_args, sampler_session, stage,
            reservoir.capacity, pending_capacity, sampler_iterations[worker], worker,
            sampler_generations[worker], actor_version, models[worker], samples, stop, deadline, budget,
            results, heartbeat, progress,
        ), name=f"sampler-{worker}")
        workers[worker] = process
        process.start()

    for worker in range(args.samplers):
        start_worker(worker)

    def publish():
        nonlocal actor, actor_version
        if stop.is_set():
            return
        actor = export_value_model(None, model, fingerprint, 1, 0, True)
        actor_version = updates
        for queue in models:
            try:
                queue.put_nowait((actor_version, actor))
            except Full:
                try:
                    queue.get(timeout=.1)
                    queue.put((actor_version, actor), timeout=.1)
                except (Empty, Full, EOFError, OSError):
                    pass

    def ingest(item, packet=True):
        nonlocal decisions, handled, forced, discarded_steps, sampled, collect_seconds, winning_added, orphan_empty_actions, latest_sampler_version, latest_sampler_iteration, dataset_peak, segmented_trajectories, queue_full_waits, queue_put_seconds, queue_delay_sum, queue_packets, queue_peak, mcts_roots, mcts_simulations, mcts_leaves, mcts_nodes, mcts_batches, mcts_targets, mcts_turn_starts, mcts_seconds, mcts_simulate_seconds, mcts_encode_seconds, mcts_inference_seconds, mcts_backup_seconds, mcts_rollout_steps, mcts_rollout_completed, mcts_rollout_invalid, mcts_rollout_seconds, mcts_timeouts
        worker, generation, version, result = item
        if generation != sampler_generations[worker]:
            return
        if packet:
            sampler_recovery_packets[worker] += 1
            if sampler_recovery_packets[worker] >= 2:
                sampler_restart_streaks[worker] = 0
        sampler_versions[worker] = version
        sampler_iterations[worker] = result["iteration"]
        latest_sampler_version = min(sampler_versions)
        latest_sampler_iteration = max(sampler_iterations)
        added, excluded, excess = dataset.add(result, args, budget - decisions)
        expert_dataset.add(result.get("expert_rows", []))
        decisions += added
        handled += excluded
        forced += excluded
        if decisions >= budget:
            stop.set()
        sampled += result["sampled_steps"]
        worker_accounted[worker] += result["sampled_steps"]
        worker_resolved[worker] += result["discarded_steps"] + sum(
            len(trajectory["rows"]) for trajectory in result["trajectories"]
        )
        discarded_steps += result["discarded_steps"]
        discarded_steps += excess
        mcts_roots += result.get("mcts_roots", 0)
        mcts_simulations += result.get("mcts_simulations", 0)
        mcts_leaves += result.get("mcts_leaves", 0)
        mcts_nodes += result.get("mcts_nodes", 0)
        mcts_batches += result.get("mcts_batches", 0)
        mcts_targets += result.get("mcts_targets", 0)
        mcts_turn_starts += result.get("mcts_turn_starts", 0)
        mcts_seconds += result.get("mcts_seconds", 0.0)
        mcts_simulate_seconds += result.get("mcts_simulate_seconds", 0.0)
        mcts_encode_seconds += result.get("mcts_encode_seconds", 0.0)
        mcts_inference_seconds += result.get("mcts_inference_seconds", 0.0)
        mcts_backup_seconds += result.get("mcts_backup_seconds", 0.0)
        mcts_rollout_steps += result.get("mcts_rollout_steps", 0)
        mcts_rollout_completed += result.get("mcts_rollout_completed", 0)
        mcts_rollout_invalid += result.get("mcts_rollout_invalid", 0)
        mcts_rollout_seconds += result.get("mcts_rollout_seconds", 0.0)
        mcts_timeouts += result.get("mcts_timeouts", 0)
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
        missing = [index for index, row in enumerate(update["rows"]) if row[4] is None]
        for start in range(0, len(missing), 128):
            indices = missing[start:start + 128]
            with torch.no_grad():
                inputs = unpack([update["rows"][index][0] for index in indices], target, model)
                logits = predict(
                    model, inputs, args.precision, args.policy_temperature,
                )[0].masked_fill(~inputs[6], -torch.inf)
                policies = logits.log_softmax(-1).cpu().numpy()
            for index, policy in zip(indices, policies):
                row = update["rows"][index]
                update["rows"][index] = (*row[:4], np.asarray(
                    policy[:packed_action_count(row[0])], np.float16,
                ))
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
        order = dataset.sample(len(dataset), rng, args.character_balanced)
        selected = order[:size]
        rows = [dataset.rows[index] for index in selected]
        expert_selected = expert_dataset.sample(args.expert_batch, rng) \
            if args.expert_weight or args.search_consistency_weight else np.empty(0, np.int64)
        expert_rows = [expert_dataset.rows[index] for index in expert_selected]
        expert_targets = [expert_dataset.targets[index] for index in expert_selected]
        expert_ids = expert_dataset.ids[expert_selected].copy()
        expert_visits_batch = expert_dataset.visits[expert_selected].copy()
        expert_depths_batch = expert_dataset.depths[expert_selected].copy()
        consistency_positions = np.asarray([
            index for index, selected in enumerate(expert_selected)
            if expert_dataset.consistencies[selected][2] < 1
        ], np.int64)
        if len(consistency_positions) > args.search_consistency_batch:
            consistency_positions = rng.choice(
                consistency_positions, args.search_consistency_batch, replace=False
            )
        search_children = []
        search_groups = []
        for group, position in enumerate(consistency_positions):
            children, weights, self_weight, terminal_value = \
                expert_dataset.consistencies[expert_selected[position]]
            search_groups.append((position, group, len(children), weights,
                                  self_weight, terminal_value))
            search_children.extend(children)
        values = {key: dataset.data[key][selected].copy() for key in dataset.data}
        replay = reservoir.sample(len(selected) // 9, rng)
        return selected, order, size, rows, expert_ids, expert_rows, expert_targets, \
            expert_visits_batch, expert_depths_batch, values, replay, search_groups, \
            search_children, packer.submit(
            prepare, rows + expert_rows + [sample[0] for sample in replay] + search_children
        )

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
            "mcts_roots": mcts_roots, "mcts_simulations": mcts_simulations,
            "mcts_leaves": mcts_leaves, "mcts_nodes": mcts_nodes,
            "mcts_batches": mcts_batches, "mcts_targets": mcts_targets,
            "mcts_turn_starts": mcts_turn_starts,
            "mcts_seconds": mcts_seconds,
            "mcts_root_fraction": mcts_roots / max(1, mcts_turn_starts),
            "mcts_roots_per_decision": mcts_roots / max(1, sampled),
            "mcts_simulations_per_root": mcts_simulations / max(1, mcts_roots),
            "mcts_simulations_per_second": mcts_simulations / max(1e-9, mcts_seconds),
            "mcts_leaf_batch_mean": mcts_leaves / max(1, mcts_batches),
            "mcts_targets_per_root": mcts_targets / max(1, mcts_roots),
            "mcts_simulate_fraction": mcts_simulate_seconds / max(1e-9, mcts_seconds),
            "mcts_encode_fraction": mcts_encode_seconds / max(1e-9, mcts_seconds),
            "mcts_inference_fraction": mcts_inference_seconds / max(1e-9, mcts_seconds),
            "mcts_backup_fraction": mcts_backup_seconds / max(1e-9, mcts_seconds),
            "mcts_rollout_steps": mcts_rollout_steps,
            "mcts_rollout_completed": mcts_rollout_completed,
            "mcts_rollout_invalid": mcts_rollout_invalid,
            "mcts_timeouts": mcts_timeouts,
            "mcts_rollout_fraction": mcts_rollout_seconds / max(1e-9, mcts_seconds),
            "expert_buffer_rows": len(expert_dataset),
            "expert_rows_seen": expert_dataset.seen,
            "expert_rows_used": expert_dataset.used,
            "expert_rows_stale": expert_dataset.stale,
            "expert_rows_evicted": expert_dataset.evicted,
            "expert_visit_mean": expert_visits / max(1, expert_dataset.used),
            "expert_depth_mean": expert_depth / max(1, expert_dataset.used),
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
            "sampler_restart_streaks": sampler_restart_streaks,
            "watchdog_dropped_steps": watchdog_dropped,
            "sample_queue_capacity": sample_capacity, "sample_queue_peak": queue_peak,
            "sample_queue_packets": queue_packets,
            "sample_queue_delay_mean": queue_delay_sum / max(1, queue_packets),
            "sample_queue_full_waits": queue_full_waits,
            "sample_queue_put_seconds": queue_put_seconds,
            "winning_reservoir": len(reservoir.rows), "winning_seen": reservoir.seen,
            "winning_character_rows": list(map(len, reservoir.by_character())),
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
            **critic_balance.report(),
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
        if args.critic_only or promotion_ready or stage + 1 >= len(STAGES):
            return
        promotion_ready, promotion_result = promotion_sample(
            promotion_episodes, args.promotion_window, args.promote_win_rate,
        )
    pending = []
    def drain_results():
        while True:
            try:
                ingest(results.get_nowait(), False)
            except (Empty, EOFError, OSError):
                return

    def watch_samplers():
        while not stop.wait(min(1., args.sampler_timeout / 10)):
            now = time.monotonic()
            if now >= deadline:
                return
            for worker, process in enumerate(workers):
                if sampler_exhausted[worker] or watchdog_pending[worker]:
                    continue
                alive = process.is_alive()
                stale = alive and now - heartbeat[worker] > args.sampler_timeout
                if not stale:
                    sampler_stale_since[worker] = None
                elif sampler_stale_since[worker] is None:
                    sampler_stale_since[worker] = now
                wedged = alive and sampler_wedged(
                    heartbeat[worker], sampler_stale_since[worker], now, args.sampler_timeout,
                )
                failed = not alive and (threaded or process.exitcode not in (None, 0))
                if not wedged and not failed:
                    continue
                watchdog_pending[worker] = True
                if threaded:
                    stop.set()
                elif wedged:
                    watchdog_terminated.add(process)
                    process.terminate(); process.join(5)
                    if process.is_alive():
                        process.kill(); process.join()
                watchdog_failures.put((
                    worker, process, wedged, None if threaded else process.exitcode,
                ))

    def recover_workers():
        nonlocal sampled, discarded_steps, watchdog_dropped
        while True:
            try:
                worker, process, wedged, exitcode = watchdog_failures.get_nowait()
            except Empty:
                return
            if process is not workers[worker]:
                watchdog_pending[worker] = False
                continue
            if threaded:
                stop.set(); sampler_exhausted[worker] = True; watchdog_pending[worker] = False
                continue
            if wedged:
                sampler_wedges[worker] += 1
            process.join(5)
            if process.is_alive():
                process.kill(); process.join()
            drain_samples()
            drain_results()
            observed = int(progress[worker])
            sampled += max(0, observed - worker_accounted[worker])
            dropped = max(0, observed - worker_resolved[worker])
            discarded_steps += dropped
            watchdog_dropped += dropped
            worker_accounted[worker] = worker_resolved[worker] = 0
            emit_event({
                "time": time.time(), "event": "sampler_failure", "worker": worker,
                "generation": sampler_generations[worker], "wedged": wedged,
                "exitcode": exitcode, "dropped": dropped,
            }, logging.WARNING)
            if (sampler_restart_streaks[worker] >= args.sampler_restarts or stop.is_set()
                    or time.monotonic() >= deadline or decisions >= budget):
                sampler_exhausted[worker] = True
                watchdog_pending[worker] = False
                continue
            sampler_restarts[worker] += 1
            sampler_restart_streaks[worker] += 1
            sampler_generations[worker] += 1
            while True:
                try:
                    models[worker].get_nowait()
                except (Empty, EOFError, OSError):
                    break
            start_worker(worker)
            watchdog_pending[worker] = False
            emit_event({
                "time": time.time(), "event": "sampler_restart", "worker": worker,
                "generation": sampler_generations[worker],
                "restarts": sampler_restarts[worker], "streak": sampler_restart_streaks[worker],
            })

    watchdog = threading.Thread(target=watch_samplers, name="sampler-watchdog", daemon=True)
    watchdog.start()
    try:
        while True:
            drain_results()
            drain_samples()
            recover_workers()
            now = time.monotonic()
            if now >= next_heartbeat_log:
                try:
                    queue_size = samples.qsize()
                except NotImplementedError:
                    queue_size = -1
                _GLOG.info(
                    "heartbeat decisions=%d sampled=%d updates=%d dataset=%d expert=%d "
                    "queue=%d sampler_version=%d sampler_age_s=%.1f",
                    decisions, sampled, updates, len(dataset), len(expert_dataset), queue_size,
                    latest_sampler_version, max(now - value for value in heartbeat[:]),
                )
                next_heartbeat_log = now + 10
            sampler_done = (stop.is_set() or all(not worker.is_alive() for worker in workers)) and samples.empty()
            if not pending:
                if len(dataset) < args.batch and not sampler_done:
                    try:
                        ingest(samples.get(timeout=.1))
                        drain_samples()
                    except (Empty, EOFError, OSError):
                        recover_workers()
                        continue
                    sampler_done = (stop.is_set() or all(not worker.is_alive() for worker in workers)) and samples.empty()
                absolute = base_decisions + handled
                boundary = min(next_report, next_save) - absolute
                stale = dataset.prune(updates, args.max_policy_lag, boundary)
                expert_dataset.prune(updates, args.expert_max_lag)
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
            (selected, order, cursor, rows, expert_ids, expert_rows, expert_targets,
             expert_visits_batch, expert_depths_batch, values, replay, search_groups, search_children,
             packed) = pending.pop(0)
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
            expert_lengths = np.asarray([
                packed_action_count(row) for row in expert_rows
            ], np.int64)
            choice_index = torch.as_tensor(
                np.cumsum(lengths) - lengths + values["action"], device=target,
            )
            flat_policy = not replay
            forward_started = time.monotonic()
            all_logits, all_critic_logits = predict(
                model, inputs, args.precision, args.policy_temperature, flat_policy=flat_policy,
            )
            forward_seconds = time.monotonic() - forward_started
            screen_forward_seconds += forward_seconds
            screen_seconds += time.monotonic() - screen_started
            forward_durations.append(forward_seconds)
            backward_seconds = 0.0
            critic_logits = all_critic_logits[:len(rows)]
            legal = inputs[6][:len(rows)]
            policy_actions = int(lengths.sum())
            expert_actions = int(expert_lengths.sum())
            logits = all_logits[:policy_actions] if flat_policy else all_logits[:len(rows)]
            expert_logits = (all_logits[policy_actions:policy_actions + expert_actions]
                             if flat_policy else
                             all_logits[len(rows):len(rows) + len(expert_rows)])
            log_ratio = (logits[choice_index] if flat_policy else
                         logits.gather(1, action[:, None]).squeeze(1)) - old
            if not args.critic_only and not (legal.sum(1) > 1).all():
                raise RuntimeError("forced action entered the dataset")
            fresh = torch.ones_like(log_ratio, dtype=torch.bool) if args.critic_only \
                else log_ratio.abs() <= args.max_log_ratio
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
                replay = []
                flat_policy = True
                unpack_started = time.monotonic()
                inputs = upload(prepare(rows + expert_rows + search_children)[0], target)
                unpack_seconds += time.monotonic() - unpack_started
                action = torch.as_tensor(values["action"], device=target)
                old = torch.as_tensor(values["old"], device=target)
                lengths = np.asarray([packed_action_count(row) for row in rows], np.int64)
                choice_index = torch.as_tensor(
                    np.cumsum(lengths) - lengths + values["action"], device=target,
                )
                forward_started = time.monotonic()
                all_logits, all_critic_logits = predict(
                    model, inputs, args.precision, args.policy_temperature, flat_policy=flat_policy,
                )
                forward_seconds += time.monotonic() - forward_started
                critic_logits = all_critic_logits[:len(rows)]
                legal = inputs[6][:len(rows)]
                policy_actions = int(lengths.sum())
                expert_actions = int(expert_lengths.sum())
                logits = all_logits[:policy_actions]
                expert_logits = all_logits[policy_actions:policy_actions + expert_actions]
                log_ratio = logits[choice_index] - old
                if not (log_ratio.abs() <= args.max_log_ratio).all():
                    raise RuntimeError("batch eligibility changed after screening")
            selected = np.asarray(selected, np.int64)
            expired = dataset.use(selected, args.priority_decay)
            removed = np.asarray(rejected + expired.tolist(), np.int64)
            dataset.ratio_dropped += len(rejected)
            if len(removed):
                dataset.discard(np.unique(removed))
            handled += len(removed)
            policy_lags.extend((updates - values["version"]).tolist())
            fresh = torch.ones_like(log_ratio, dtype=torch.bool)
            mask = torch.ones_like(log_ratio)
            fresh_count = mask.sum()
            denominator = fresh_count.clamp_min(1)
            safe_log_ratio = torch.where(fresh, log_ratio, torch.zeros_like(log_ratio))
            ratio = safe_log_ratio.exp()
            kl = ((ratio - 1 - safe_log_ratio) * mask).sum().div(denominator).detach()
            fresh_cpu = fresh.detach().cpu().numpy()
            fresh_rows = int(fresh_count)
            attempted += len(rows)
            kl_value = float(kl)
            _, entropy_weight, _ = curriculum_weights(
                stage, stage_decisions + handled, auxiliary_decisions + handled, args, progress_active
            )
            batch_target = torch.as_tensor(values["critic_target"], device=target)
            critic_weights = critic_balance.weights(
                values["character"], values["phase"], values["canonical"],
            )
            critic_weights = torch.as_tensor(critic_weights, device=target)
            category_loss = -batch_target * critic_logits.log_softmax(-1)
            value_loss = (category_loss.sum(1) * critic_weights).mean()
            critic_loss = args.value_weight * value_loss
            critic_parameters = tuple(model.critic.parameters())
            critic_balance.record_loss(
                values["critic_target"].astype(np.float64),
                (category_loss.detach() * critic_weights[:, None]).cpu().numpy(),
            )
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
            policy_update_attempts += int(not args.critic_only)
            if not args.critic_only and kl_value > args.target_kl:
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
            characters = torch.as_tensor(values["character"], device=target)
            batch_advantage = advantages
            for character in range(5):
                member = fresh & (characters == character)
                member_float = member.to(advantages.dtype)
                count = member_float.sum().clamp_min(1)
                mean = (advantages * member_float).sum() / count
                std = (((advantages - mean).square() * member_float).sum() / count).sqrt()
                batch_advantage = torch.where(
                    member, (advantages - mean) / (std + 1e-8), batch_advantage,
                )
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
            expert_loss = logits.sum() * 0
            expert_entropy = expert_loss
            expert_count = len(expert_rows)
            if expert_count and args.expert_weight:
                if flat_policy:
                    expert_target = np.concatenate(expert_targets).astype(np.float32)
                else:
                    expert_target = np.zeros(expert_logits.shape, np.float32)
                    for row, target_distribution in enumerate(expert_targets):
                        expert_target[row, :len(target_distribution)] = target_distribution
                expert_target = torch.as_tensor(expert_target, device=target)
                positive = expert_target > 0
                expert_loss = -(expert_target[positive] * expert_logits[positive]).sum() \
                    / expert_count
                expert_entropy = -(expert_target[positive] * expert_target[positive].log()).sum() \
                    / expert_count
            search_consistency_loss = critic_logits.sum() * 0
            if search_groups:
                critic_values = critic_expected(critic_probabilities(all_critic_logits))
                child_offset = len(rows) + len(expert_rows) + len(replay)
                consistency_losses = []
                for position, _group, count, weights, self_weight, terminal_value in search_groups:
                    parent = critic_values[len(rows) + position]
                    children = critic_values[child_offset:child_offset + count]
                    child_offset += count
                    expected = self_weight * parent + terminal_value
                    if count:
                        expected = expected + (children * torch.as_tensor(
                            weights, device=target
                        )).sum()
                    consistency_losses.append((parent - expected).square())
                search_consistency_loss = torch.stack(consistency_losses).mean()
            loss = policy_loss + critic_loss - entropy_weight * entropy \
                + args.expert_weight * expert_loss \
                + args.search_consistency_weight * search_consistency_loss
            ppo_head_grad = expert_head_grad = expert_ppo_grad_cosine = None
            if expert_count and args.expert_weight:
                policy_parameters = tuple(model.policy.parameters())
                ppo_gradients = torch.autograd.grad(
                    policy_loss, policy_parameters, retain_graph=True, allow_unused=True
                )
                expert_gradients = torch.autograd.grad(
                    args.expert_weight * expert_loss, policy_parameters,
                    retain_graph=True, allow_unused=True,
                )
                pairs = [(left.float(), right.float()) for left, right in zip(
                    ppo_gradients, expert_gradients
                ) if left is not None and right is not None]
                ppo_head_grad = torch.stack([left.square().sum() for left, _ in pairs]).sum().sqrt()
                expert_head_grad = torch.stack([
                    right.square().sum() for _, right in pairs
                ]).sum().sqrt()
                expert_ppo_grad_cosine = torch.stack([
                    (left * right).sum() for left, right in pairs
                ]).sum() / (ppo_head_grad * expert_head_grad).clamp_min(1e-12)
            replay_valid = replay_eligible = eligible_cpu = valid_cpu = None
            if replay:
                replay_start = len(rows) + len(expert_rows)
                replay_logits = all_logits[replay_start:]
                replay_value = critic_probabilities(all_critic_logits[replay_start:])[:, -1]
                replay_legal = inputs[6][replay_start:]
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
                replay_advantage = (1 - replay_value).detach()
                winning_loss = -args.winning_loss_weight * (torch.minimum(
                    replay_ratio * replay_advantage,
                    replay_ratio.clamp(1 - args.clip, 1 + args.clip) * replay_advantage,
                ) * replay_weight).sum() / replay_capacity.clamp_min(1)
                loss = loss + winning_loss
                replay_kl_mean = (replay_kl * replay_weight).sum() / replay_denominator
            backward_started = time.monotonic()
            optimizer.zero_grad(set_to_none=True); loss.backward()
            if args.critic_only:
                critic_only_step(model, optimizer); critic_only_updates += 1
                accepted, proposals, post_log_ratio = True, [0.], log_ratio.detach()
            elif args.target_kl >= 1:
                nn.utils.clip_grad_norm_(model.parameters(), .5)
                optimizer.step()
                accepted, proposals, post_log_ratio = True, [], log_ratio.detach()
            else:
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
            post_kl_retries += max(0, len(proposals) - 1)
            post_kl_retry_depth = max(post_kl_retry_depth, max(0, len(proposals) - 1))
            if not accepted:
                dataset.post_kl_dropped += fresh_rows
                post_kl_discarded_updates += 1
                optimizer.zero_grad(set_to_none=True)
                retry_logits = predict(
                    model, inputs, args.precision, args.policy_temperature,
                )[1][:len(rows)]
                retry_critic_loss = args.value_weight * (
                    -batch_target * retry_logits.log_softmax(-1)
                ).sum(1).mul(critic_weights).mean()
                retry_critic_loss.backward(inputs=critic_parameters)
                critic_only_step(model, optimizer, critic_parameters); publish()
                critic_only_updates += 1
                update_elapsed = time.monotonic() - update_started
                update_seconds += update_elapsed
                update_durations.append(update_elapsed)
                backward_durations.append(time.monotonic() - backward_started)
                continue
            post_kl = proposals[-1] if proposals else kl_value
            post_ratio = post_log_ratio.exp()
            observed_kl = post_kl
            observed_clip = float(
                (((post_ratio - 1).abs() > args.clip) * fresh).sum() / denominator
            )
            losses["post_kl"].append(post_kl)
            accepted_pre_kl_sum += kl_value
            accepted_post_kl_sum += post_kl
            clip_fraction = (((ratio - 1).abs() > args.clip).to(mask.dtype) * mask).sum().div(denominator).detach()
            losses["mean_advantage"].append(float(np.mean(values["advantage"][fresh_cpu])))
            losses["policy_loss"].append(policy_loss.detach())
            if expert_count and args.expert_weight:
                losses["expert_loss"].append(expert_loss.detach())
                losses["expert_entropy"].append(expert_entropy.detach())
                losses["expert_kl"].append((expert_loss - expert_entropy).detach())
                losses["expert_rows"].append(expert_count)
                losses["ppo_policy_head_grad_norm"].append(ppo_head_grad.detach())
                losses["expert_policy_head_grad_norm"].append(expert_head_grad.detach())
                losses["expert_ppo_grad_ratio"].append(
                    (expert_head_grad / ppo_head_grad.clamp_min(1e-12)).detach()
                )
                losses["expert_ppo_grad_cosine"].append(expert_ppo_grad_cosine.detach())
            probabilities = critic_probabilities(critic_logits.detach())
            losses["critic_loss"].append(value_loss.detach())
            if search_groups:
                losses["search_consistency_loss"].append(search_consistency_loss.detach())
            losses["critic_expected"].append(critic_expected(probabilities).mean())
            losses["critic_win_probability"].append(probabilities[:, -1].mean())
            losses["entropy"].append(entropy.detach())
            losses["entropy_weight"].append(entropy_weight)
            losses["kl"].append(kl); losses["clip_fraction"].append(clip_fraction)
            trained += fresh_rows; updates += 1
            if expert_count:
                expert_visits += int(expert_visits_batch.sum())
                expert_depth += int(expert_depths_batch.sum())
                expert_dataset.used += expert_count
                expert_dataset.discard_ids(expert_ids)
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
            if target.type == "mps":
                torch.mps.empty_cache()
            backward_seconds = time.monotonic() - backward_started
            backward_durations.append(backward_seconds)
            update_elapsed = time.monotonic() - update_started
            update_seconds += update_elapsed
            update_durations.append(update_elapsed)
            _GLOG.info(
                "optimizer_update update=%d rows=%d expert_rows=%d replay=%d "
                "policy_loss=%.6g expert_loss=%.6g ppo_head_grad=%.6g expert_head_grad=%.6g "
                "expert_grad_cosine=%.6g critic_loss=%.6g search_consistency_loss=%.6g "
                "critic_expected=%.6g "
                "critic_win_probability=%.6g entropy=%.6g "
                "pre_kl=%.6g post_kl=%.6g seconds=%.3f",
                updates, fresh_rows, expert_count, len(replay), float(policy_loss.detach()),
                float(expert_loss.detach()),
                float(ppo_head_grad.detach()) if ppo_head_grad is not None else 0,
                float(expert_head_grad.detach()) if expert_head_grad is not None else 0,
                float(expert_ppo_grad_cosine.detach())
                if expert_ppo_grad_cosine is not None else 0,
                float(value_loss.detach()), float(search_consistency_loss.detach()),
                float(critic_expected(probabilities).mean()),
                float(probabilities[:, -1].mean()),
                float(entropy.detach()),
                kl_value, post_kl, update_elapsed,
            )
            if update_elapsed > 5:
                packed = rows + expert_rows + [sample[0] for sample in replay]
                represented_actions = max(packed_action_count(row) for row in packed)
                emit_event({"time": time.time(), "event": "slow_update", "metrics": {
                    "seconds": update_elapsed,
                    "fresh": len(rows), "replay": len(replay),
                    "state_tokens_max": max(packed_state_count(row) for row in packed),
                    "represented_actions_max": represented_actions,
                    "state_attention_pairs": sum((packed_state_count(row) + 1) ** 2 for row in packed),
                    "legal_actions_max": max(packed_legal_count(row) for row in packed),
                    "unpack_seconds": unpack_seconds,
                    "forward_seconds": forward_seconds, "backward_seconds": backward_seconds,
                }}, logging.WARNING)
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
        watchdog.join()
        recover_workers()
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
            observed = int(progress[index])
            sampled += max(0, observed - worker_accounted[index])
            dropped = max(0, observed - worker_resolved[index])
            discarded_steps += dropped
            watchdog_dropped += dropped * (worker in terminated)
        if not threaded:
            for queue in (*models, samples, results):
                queue.cancel_join_thread()
                queue.close()
    failed = [] if threaded else [worker.exitcode for worker in started_workers
                                  if worker not in terminated
                                  and worker not in watchdog_terminated and worker.exitcode]
    if failed:
        raise RuntimeError(f"sampler processes failed: {failed}")
    assert handled == decisions and not len(dataset)
    unresolved = sampled - decisions - discarded_steps
    assert unresolved >= 0, (sampled, decisions, discarded_steps)
    discarded_steps += unresolved
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
        "mcts_roots": mcts_roots, "mcts_simulations": mcts_simulations,
        "mcts_leaves": mcts_leaves, "mcts_nodes": mcts_nodes,
        "mcts_batches": mcts_batches, "mcts_targets": mcts_targets,
        "mcts_turn_starts": mcts_turn_starts,
        "mcts_seconds": mcts_seconds,
        "mcts_root_fraction": mcts_roots / max(1, mcts_turn_starts),
        "mcts_roots_per_decision": mcts_roots / max(1, sampled),
        "mcts_simulations_per_root": mcts_simulations / max(1, mcts_roots),
        "mcts_simulations_per_second": mcts_simulations / max(1e-9, mcts_seconds),
        "mcts_leaf_batch_mean": mcts_leaves / max(1, mcts_batches),
        "mcts_targets_per_root": mcts_targets / max(1, mcts_roots),
        "mcts_simulate_fraction": mcts_simulate_seconds / max(1e-9, mcts_seconds),
        "mcts_encode_fraction": mcts_encode_seconds / max(1e-9, mcts_seconds),
        "mcts_inference_fraction": mcts_inference_seconds / max(1e-9, mcts_seconds),
        "mcts_backup_fraction": mcts_backup_seconds / max(1e-9, mcts_seconds),
        "mcts_rollout_steps": mcts_rollout_steps,
        "mcts_rollout_completed": mcts_rollout_completed,
        "mcts_rollout_invalid": mcts_rollout_invalid,
        "mcts_timeouts": mcts_timeouts,
        "mcts_rollout_fraction": mcts_rollout_seconds / max(1e-9, mcts_seconds),
        "expert_buffer_rows": len(expert_dataset),
        "expert_rows_seen": expert_dataset.seen,
        "expert_rows_used": expert_dataset.used,
        "expert_rows_stale": expert_dataset.stale,
        "expert_rows_evicted": expert_dataset.evicted,
        "expert_visit_mean": expert_visits / max(1, expert_dataset.used),
        "expert_depth_mean": expert_depth / max(1, expert_dataset.used),
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
        "sampler_restart_streaks": sampler_restart_streaks,
        "watchdog_dropped_steps": watchdog_dropped,
        "sample_queue_capacity": sample_capacity, "sample_queue_peak": queue_peak,
        "sample_queue_packets": queue_packets,
        "sample_queue_delay_mean": queue_delay_sum / max(1, queue_packets),
        "sample_queue_full_waits": queue_full_waits,
        "sample_queue_put_seconds": queue_put_seconds,
        "winning_reservoir": len(reservoir.rows), "winning_seen": reservoir.seen,
        "winning_character_rows": list(map(len, reservoir.by_character())),
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
        **critic_balance.report(),
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


def evaluate_search_policies(args):
    model, checkpoint = load(args.checkpoint, torch.device("cpu")); model.eval()
    ascension, bonus = STAGES[checkpoint["stage"]]
    policy = export_value_model(None, model, checkpoint["fingerprint"], 1, 0, True)
    modes = []
    for mode in args.modes:
        started = time.monotonic(); characters = []
        totals = dict.fromkeys((
            "mcts_roots", "mcts_simulations", "mcts_targets", "mcts_rollout_steps",
            "mcts_rollout_completed", "mcts_rollout_invalid", "mcts_rollout_micros",
            "search_hits", "search_misses",
            "exact_calls", "exact_states", "exact_transitions", "exact_rng_transitions",
            "exact_rng_calls", "exact_failures",
        ), 0)
        for character in args.characters:
            outcomes = []; floors = []; caps = 0
            for start in range(0, args.runs, args.batch):
                count = min(args.batch, args.runs - start)
                env = sts2_sim.Batch(
                    count, args.seed + character * 1_000_000 + start,
                    character, ascension=ascension,
                )
                env.set_training_bonus(bonus); env.load_policy(policy)
                active = np.ones(count, bool); steps = np.zeros(count, np.int32)
                combat_steps = np.zeros(count, np.int32); turn_depth = np.zeros(count, np.int32)
                previous_turn = np.full(count, -1, np.int32); targets = {}
                for _ in range(args.max_steps):
                    if not active.any():
                        break
                    stats = env.stats()
                    in_combat = np.asarray([row[4] == 1 for row in stats])
                    turns = np.asarray([row[5] if row[4] == 1 else -1 for row in stats])
                    fresh_turn = active & in_combat & (turns != previous_turn)
                    turn_depth[fresh_turn] = 0
                    result = env.policy(
                        args.policy_temperature, False, False,
                        mcts_fraction=1 if mode not in ("policy", "exact") else 0,
                        mcts_simulations=args.simulations if mode not in ("policy", "exact") else 0,
                        mcts_boss_simulations=args.simulations
                        if mode not in ("policy", "exact") else 0,
                        mcts_turns=0 if mode.startswith("combat") else 1,
                        mcts_max_depth=args.max_depth,
                        mcts_batch_size=args.search_batch, mcts_min_visits=args.min_visits,
                        mcts_max_targets=args.max_targets,
                        mcts_prior_temperature=args.prior_temperature,
                        mcts_q_temperature=args.q_temperature,
                        mcts_exploration=args.exploration,
                        mcts_heuristic=mode.endswith("heuristic"),
                    )
                    choices = np.asarray(result[1], np.int64); rows = result[4]
                    if mode != "policy" and mode != "exact":
                        search = np.asarray(result[6], np.int64)
                        totals["mcts_roots"] += int(search[0])
                        totals["mcts_simulations"] += int(search[1])
                        totals["mcts_targets"] += int(search[5])
                        totals["mcts_rollout_steps"] += int(search[12])
                        totals["mcts_rollout_completed"] += int(search[13])
                        totals["mcts_rollout_invalid"] += int(search[14])
                        totals["mcts_rollout_micros"] += int(search[15])
                        for row, target, _visits, depth, *_ in result[5]:
                            targets[bytes(row), int(depth)] = int(np.argmax(target))
                    elif mode == "exact":
                        indices = [int(index) for index in np.flatnonzero(fresh_turn)
                                   if packed_legal_count(rows[index]) > 1]
                        exact = env.exact_choices(
                            indices, 1, args.max_depth, args.exact_samples,
                            args.max_exact_states,
                            args.seed + character * 1_000_000 + start, True,
                        )
                        for index, row in zip(indices, exact):
                            (_choice, _value, states, transitions, rng_transitions,
                             rng, plan, error) = row
                            totals["exact_calls"] += 1
                            totals["exact_states"] += states
                            totals["exact_transitions"] += transitions
                            totals["exact_rng_transitions"] += rng_transitions
                            totals["exact_rng_calls"] += rng
                            if error:
                                totals["exact_failures"] += 1
                            else:
                                for packed, choice, depth in plan:
                                    targets[bytes(packed), int(depth)] = choice
                    if mode != "policy":
                        for index in np.flatnonzero(active & in_combat):
                            choice = targets.get((bytes(rows[index]), int(turn_depth[index])))
                            if choice is None:
                                totals["search_misses"] += packed_legal_count(rows[index]) > 1
                            else:
                                choices[index] = choice; totals["search_hits"] += 1
                    combat_steps = np.where(active & in_combat, combat_steps + 1, 0)
                    turn_depth[active & in_combat] += 1
                    previous_turn = np.where(active & in_combat, turns, -1)
                    _, done, _ = env.step(choices.tolist(), active.tolist())
                    done = np.asarray(done, bool); steps += active
                    final = env.stats()
                    still_combat = np.asarray([row[4] == 1 for row in final])
                    capped = active & ~done & (
                        (steps >= args.max_steps)
                        | ((combat_steps >= args.max_combat_steps) & still_combat)
                    )
                    for index in np.flatnonzero(done | capped):
                        outcomes.append(int(done[index] and final[index][4] == 12))
                        floors.append(int((final[index][0] - 1) * 17 + final[index][1]))
                    caps += int(capped.sum())
                    finished = np.flatnonzero(done | capped).tolist()
                    active &= ~(done | capped)
                    if finished:
                        env.reset(finished, 0)
            wins = sum(outcomes); boss_entries = sum(
                won or floor >= 51 for won, floor in zip(outcomes, floors)
            )
            characters.append({
                "character": character, "runs": args.runs, "wins": wins,
                "win_rate": wins / args.runs, "floor_mean": float(np.mean(floors)),
                "boss_entries": boss_entries, "boss_entry_rate": boss_entries / args.runs,
                "boss_conversion": wins / max(1, boss_entries), "caps": caps,
                "outcomes": outcomes, "floors": floors,
            })
            print(json.dumps({"mode": mode, "character": character, "wins": wins,
                              "floor_mean": characters[-1]["floor_mean"],
                              "seconds": time.monotonic() - started}), flush=True)
        wins = sum(row["wins"] for row in characters); runs = len(args.characters) * args.runs
        boss_entries = sum(row["boss_entries"] for row in characters)
        modes.append({
            "mode": mode, "runs": runs, "wins": wins, "win_rate": wins / runs,
            "floor_mean": float(np.mean([floor for row in characters for floor in row["floors"]])),
            "boss_entries": boss_entries, "boss_entry_rate": boss_entries / runs,
            "boss_conversion": wins / max(1, boss_entries), "caps": sum(row["caps"] for row in characters),
            "seconds": time.monotonic() - started, "characters": characters, **totals,
        })
        print(json.dumps({key: modes[-1][key] for key in (
            "mode", "runs", "wins", "win_rate", "floor_mean", "boss_entries",
            "boss_conversion", "caps", "seconds", "search_hits", "search_misses",
            "mcts_simulations", "mcts_rollout_steps", "mcts_rollout_completed",
            "mcts_rollout_invalid", "mcts_rollout_micros", "exact_calls", "exact_states",
            "exact_failures",
        )}), flush=True)
    report = {
        "checkpoint": args.checkpoint, "step": checkpoint["decisions"],
        "stage": {"index": checkpoint["stage"], "ascension": ascension, "bonus": bonus},
        "settings": {key: value for key, value in vars(args).items() if key != "command"},
        "modes": modes,
    }
    immutable_json(Path(args.output), report)


def critic_metrics(rows):
    probability = np.asarray([row["critic_probability"] for row in rows], np.float32)
    category = np.asarray([row["terminal_category"] for row in rows], np.int64)
    selected = probability[np.arange(len(rows)), category]
    result = {
        "states": len(rows),
        "categorical_nll": float(-np.log(selected.clip(1e-12)).mean()),
        "categorical_brier": float(np.mean(np.square(probability).sum(1) + 1 - 2 * selected)),
    }
    for name, target_key, prediction_key, scale in (
        ("win", "win", "win_prediction", 1),
        ("floor", "terminal_floor", "floor_prediction", CATEGORIES - 1),
    ):
        target = np.asarray([row[target_key] for row in rows], np.float32)
        prediction = np.asarray([row[prediction_key] for row in rows], np.float32)
        error = (prediction - target) * scale
        correlation = float(np.corrcoef(target, prediction)[0, 1]) \
            if len(rows) > 1 and target.std() > 1e-7 and prediction.std() > 1e-7 else None
        correlation = correlation if correlation is None or math.isfinite(correlation) else None
        result[name] = {
            "target_mean": float(target.mean()), "prediction_mean": float(prediction.mean()),
            "target_std": float(target.std()), "prediction_std": float(prediction.std()),
            "bias": float(error.mean()), "mae": float(np.abs(error).mean()),
            "rmse": float(np.sqrt(np.mean(error ** 2))), "correlation": correlation,
        }
    return result


def critic_diagnostics(args):
    model, checkpoint = load(args.checkpoint, torch.device("cpu")); model.eval()
    ascension, bonus = STAGES[checkpoint["stage"]]
    policy = export_value_model(None, model, checkpoint["fingerprint"], 1, 0, True)
    states = []
    transitions = []
    capped_episodes = 0
    for character in range(5):
        for start in range(0, args.runs, args.batch):
            count = min(args.batch, args.runs - start)
            env = sts2_sim.Batch(
                count, args.seed + character * 1_000_000 + start,
                character, ascension=ascension,
            )
            env.set_training_bonus(bonus); env.load_policy(policy)
            active = np.ones(count, bool)
            histories = [[] for _ in range(count)]
            steps = np.zeros(count, np.int32)
            combat_steps = np.zeros(count, np.int32)
            for _ in range(args.max_steps):
                if not active.any():
                    break
                stats = env.stats()
                result = env.policy(args.policy_temperature, True, True)
                critic_probability = np.asarray(result[3], np.float32)
                win = critic_probability[:, -1]
                floor = critic_probability @ np.arange(CATEGORIES) / (CATEGORIES - 1)
                for index in np.flatnonzero(active):
                    row = stats[index]
                    histories[index].append({
                        "character": character, "phase": int(row[4]),
                        "floor": int(row[9]),
                        "reward_screen": row[4] == 2,
                        "win_prediction": float(win[index]),
                        "floor_prediction": float(floor[index]),
                        "critic_probability": critic_probability[index].tolist(),
                    })
                done = np.asarray(result[8], bool)
                final = result[9]
                legal = np.asarray(result[10], bool)
                in_combat = np.asarray(result[11], bool)
                combat_steps = np.where(active & in_combat, combat_steps + 1, 0)
                steps += active
                still_combat = np.asarray([row[4] == 1 for row in final])
                capped = active & ~done & (
                    (steps >= args.max_steps)
                    | ((combat_steps >= args.max_combat_steps) & still_combat)
                    | ~legal
                )
                for index in np.flatnonzero(active & done):
                    terminal_category = CATEGORIES - 1 if final[index][4] == 12 else int(final[index][9])
                    path = histories[index]
                    successor = [state["floor_prediction"] for state in path[1:]] \
                        + [terminal_category / (CATEGORIES - 1)]
                    transitions.extend({
                        "floor": state["floor"],
                        "residual": (state["floor_prediction"] - next_value) * (CATEGORIES - 1),
                    } for state, next_value in zip(path, successor))
                    states.extend(state | {
                        "win": int(final[index][4] == 12),
                        "terminal_floor": terminal_category / (CATEGORIES - 1),
                        "terminal_category": terminal_category,
                    } for state in histories[index])
                capped_episodes += int(capped.sum())
                finished = np.flatnonzero(done | capped | ~legal).tolist()
                active &= ~(done | capped)
                if finished:
                    env.reset(finished, 0)
    phase_names = ("map", "combat", "rewards", "shop", "rest", "event", "remove",
                   "upgrade", "transform", "enchant", "choose_cards", "choose_bundles")
    grouped = {}
    for name, key in (
        ("character", lambda row: row["character"]),
        ("phase", lambda row: phase_names[row["phase"]]),
        ("floor", lambda row: row["floor"]),
        ("reward_screen", lambda row: row["reward_screen"]),
    ):
        values = {}
        for row in states:
            values.setdefault(str(key(row)), []).append(row)
        grouped[name] = {value: critic_metrics(rows) for value, rows in values.items()}
    residual = np.asarray([row["residual"] for row in transitions], np.float32)
    transition_consistency = {"transitions": len(residual)}
    if len(residual):
        transition_consistency |= {
            "bias": float(residual.mean()), "mae": float(np.abs(residual).mean()),
            "rmse": float(np.sqrt(np.mean(residual ** 2))),
            "p95_absolute": float(np.quantile(np.abs(residual), .95)),
        }
    report = {
        "checkpoint": args.checkpoint, "step": checkpoint["decisions"],
        "stage": {"index": checkpoint["stage"], "ascension": ascension, "bonus": bonus},
        "runs_per_character": args.runs, "states": len(states),
        "capped_episodes": capped_episodes, "overall": critic_metrics(states),
        "groups": grouped,
        "transition_consistency": transition_consistency,
    }
    immutable_json(Path(args.output), report)
    print(json.dumps({key: report[key] for key in (
        "checkpoint", "step", "stage", "runs_per_character", "states",
        "capped_episodes", "overall", "transition_consistency",
    )}, indent=2))


def immutable_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as output:
        json.dump(value, output, indent=2, sort_keys=True)


def atomic_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True))
    temporary.replace(path)


def atomic_live(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    data = json.dumps(value, separators=(",", ":")).replace("</", "<\\/")
    temporary.write_text(f"window.spirefyshLive={data};")
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
                    stage_decisions, critic_balance=None, replace=False):
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
            "critic_balance": critic_balance.state_dict() if critic_balance else None,
            "winning_reservoir": reservoir.state_dict() if reservoir else None,
            "model": model.state_dict(), "optimizer": optimizer.state_dict(),
            "torch_rng": torch.get_rng_state(),
        }, output)
    if replace:
        target.replace(path)
    return sha256_file(path)


def optimizer_compatible(state, parameter_count):
    return len([parameter for group in state["param_groups"] for parameter in group["params"]]) \
        == parameter_count


def optimizer_groups(model, policy_multiplier, critic_multiplier):
    parameters = list(model.parameters())
    policy = {id(parameter) for parameter in model.policy.parameters()}
    critic = {id(parameter) for parameter in model.critic.parameters()}
    indices = [
        [index for index, parameter in enumerate(parameters)
         if id(parameter) not in policy | critic],
        [index for index, parameter in enumerate(parameters) if id(parameter) in policy],
        [index for index, parameter in enumerate(parameters) if id(parameter) in critic],
    ]
    return [
        {"params": [parameters[index] for index in group], "lr_scale": scale}
        for group, scale in zip(indices, (1., policy_multiplier, critic_multiplier))
    ], indices


def repartition_optimizer(state, index_groups):
    groups = state["param_groups"]
    parameter_count = sum(map(len, index_groups))
    saved = [parameter for group in groups for parameter in group["params"]]
    if len(saved) != parameter_count:
        return False
    if len(groups) == 1:
        by_index = saved
        templates = groups * len(index_groups)
    elif len(groups) == len(index_groups) and all(
        len(group["params"]) == len(indices) for group, indices in zip(groups, index_groups)
    ):
        by_index = [None] * parameter_count
        for group, indices in zip(groups, index_groups):
            for index, parameter in zip(indices, group["params"]):
                by_index[index] = parameter
        templates = groups
    else:
        return False
    state["param_groups"] = [
        copy.deepcopy(template) | {
            "params": [by_index[index] for index in indices], "lr_scale": scale,
        }
        for template, indices in zip(templates, index_groups)
        for scale in [template.get("lr_scale", 1.)]
    ]
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
             "metrics": dict(reports[key]["metrics"]), "_written": reports[key].get("_written")}
            for index, key in enumerate(sorted(reports), 1)
        ]
        for row in report_rows:
            floors = row["metrics"].get("trajectory_floors", [])
            floor_groups = [[] for _ in range(5)]
            character = 0; previous = -math.inf
            for floor in floors:
                character = min(4, max(0, int(floor[2]))) if len(floor) > 2 \
                    else min(4, character + int(floor[0] < previous))
                floor_groups[character].append(floor)
                previous = floor[0]
            row["metrics"]["trajectory_floors"] = [
                floor for group in floor_groups
                for floor in group[::max(1, math.ceil(len(group) / 8))]
            ]
            for key in tuple(row["metrics"]):
                if key.startswith("critic_preweight_") or key in (
                    "characters", "critic_postweight_loss_mass",
                ):
                    row["metrics"].pop(key)
        version_groups = {}
        for row in report_rows:
            marker = row["description"].partition("Continuous V")[2].partition(" ")[0]
            version = int(marker) if marker.isdigit() else manifest.get("model_version", manifest.get("version", 0))
            version_groups.setdefault(version, []).append(row)
        for version, version_reports in version_groups.items():
            for row in version_reports[1:]:
                row.pop("description", None); row.pop("pipeline", None)
            version_manifest = manifest | manifest.get("version_history", {}).get(str(version), {})
            version_manifest["model_version"] = version
            if version == MODEL_VERSION:
                saved = version_manifest.get("architecture", {})
                current = Agent(
                    version_manifest["layout"],
                    *(saved.get(key, default) for key, default in zip(
                        ("width", "layers", "heads", "feedforward", "head_width"),
                        (128, 4, 8, 384, 128),
                    )),
                    pooling=saved.get("pooling"),
                )
                version_manifest |= {
                    "architecture": architecture(current), "change": CHANGE,
                    "feature_version": FEATURE_VERSION,
                    "parameters": sum(parameter.numel() for parameter in current.parameters()),
                }
            if len(version_groups) > 1:
                sessions = [row for row in manifest.get("sessions", []) if row["step"] <= version_reports[-1]["step"]]
                version_manifest["sessions"] = sessions[-1:] or manifest.get("sessions", [])[:1]
            low, high = version_reports[0]["step"], version_reports[-1]["step"]
            key = name if len(version_groups) == 1 else f"{name}/V{version}"
            live = str((run / "live.js").relative_to(target)) \
                if version == manifest.get("model_version") else None
            runs[key] = {
                "version": version, "manifest": version_manifest,
                "source": manifest.get("source") if len(version_groups) == 1 else None,
                "reports": version_reports,
                "promotions": [promotions[key] for key in sorted(promotions) if low <= key <= high],
                "best": best if version == manifest.get("model_version") else None, "live": live,
            }
            if live:
                atomic_live(run / "live.js", {"version": version, "report": version_reports[-1]})
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
function layout(percent=false,range,history=[],run){const title=xaxis.value==='time'?'Wall-clock time (minutes)':xaxis.value==='decisions'?'# decisions':'Optimizer steps';return{template:'plotly_dark',paper_bgcolor:'rgba(0,0,0,0)',plot_bgcolor:'rgba(0,0,0,0)',margin:{l:62,r:18,t:12,b:52},hovermode:'closest',showlegend:true,uirevision:`${versionSelect.value}:${xaxis.value}`,shapes:stageLines(history,run),xaxis:{title,gridcolor:'#303a49'},yaxis:{gridcolor:'#303a49',tickformat:percent?'.0%':undefined,range,zerolinecolor:'#8794a8'}}}
function plot(id,points,{range,percent=false,tozero=false,history=[],run}={}){const traces=[{x:points.map(point=>point.x),y:points.map(point=>point.y),mode:'lines+markers',name:'raw',line:{color:'#6fb1ff',width:2},marker:{color:'#6fb1ff',size:6,opacity:.8},hovertemplate:'x %{x}<br>y %{y:.5g}<extra></extra>'}];if(smooth.checked&&points.length>1){const line=emaLine(points);traces.push({x:line.map(point=>point.x),y:line.map(point=>point.y),mode:'lines',name:`EMA α=${Number(ema.value)||.2}`,line:{color:'#ffb454',width:4},hovertemplate:'EMA %{y:.5g}<extra></extra>'})}const options=layout(percent,range,history,run);if(tozero)options.yaxis.rangemode='tozero';Plotly.react(id,traces,options,config)}
function floorPlot(history,run){const training=run.manifest.sessions?.at(-1)?.training||run.manifest.training||{},envs=training.envs||1,points=[];for(const report of history){let inferred=0,last=-Infinity;for(const row of report.metrics.trajectory_floors||[]){const [iteration,floor,stored]=row;if(stored===undefined&&iteration<last)inferred++;const character=stored??Math.min(inferred,4);points.push({x:xaxis.value==='updates'?report._updates:xaxis.value==='decisions'?iteration*envs:(report.metrics.seconds||0)/60,y:floor,character,updates:report._updates,step:report.step});last=iteration}}for(let i=points.length-1;i>0;i--){const j=Math.floor(Math.random()*(i+1));[points[i],points[j]]=[points[j],points[i]]}const trajectory={x:points.map(point=>point.x),y:points.map(point=>point.y),customdata:points.map(point=>[characterNames[point.character],point.updates,point.step]),mode:'markers',showlegend:false,marker:{color:points.map(point=>characterColors[point.character]),size:6,opacity:.5},hovertemplate:'%{customdata[0]}<br>floor %{y}<br>optimizer steps %{customdata[1]:,}<br>report decisions %{customdata[2]:,}<extra></extra>'},legend=characterNames.map((name,character)=>({x:[null],y:[null],mode:'markers',name,hoverinfo:'skip',marker:{color:characterColors[character],size:7,opacity:.5}})),options=layout(false,[0,52],history,run);options.yaxis.title='Terminal floor';Plotly.react('floor',[trajectory,...legend],options,config)}
function stagePlot(id,history,key,color,run){const rows=history.filter(row=>Number.isFinite(Number(row.stage?.[key]))).map(row=>({x:x(row),y:row.stage[key]}));for(const transition of stageTransitions(history,run))if(Number.isFinite(Number(transition.stage?.[key])))rows.push({x:transition.x,y:transition.stage[key]});rows.sort((left,right)=>left.x-right.x);const options=layout(false,undefined,history,run);options.yaxis={...options.yaxis,title:key==='ascension'?'Ascension':'Bonus strength',rangemode:'tozero',dtick:key==='ascension'?1:4};Plotly.react(id,[{x:rows.map(row=>row.x),y:rows.map(row=>row.y),mode:'lines+markers',name:key==='ascension'?'Ascension':'Bonus strength',line:{color,width:3,shape:'hv'},marker:{color,size:6},hovertemplate:`${key==='ascension'?'ascension':'bonus'} %{y}<extra></extra>`}],options,config)}
function promotionSummary(row){if(!row)return '<p>No promotion check yet.</p>';const characters=row.result?.characters||[],rows=characters.map(item=>`<tr><td>${characterNames[item.character]??`Character ${item.character}`}</td><td>${item.wins}/${item.runs}</td><td>${(100*item.wins/item.runs).toFixed(1)}%</td><td>${Number(item.floor_mean).toFixed(2)}</td><td>${item.caps}</td></tr>`).join('');return `<p class="${row.promoted?'yes':'no'}">${row.promoted?'Promoted':'Stayed at current stage'} · threshold ${(100*row.threshold).toFixed(0)}% per character · seed ${row.seed}</p><table><thead><tr><th>Character</th><th>Wins</th><th>Rate</th><th>Mean floor</th><th>Caps</th></tr></thead><tbody>${rows}</tbody></table>`}
function saveDashboardState(){const views={};document.querySelectorAll('.plot').forEach(node=>{const view={};if(node._fullLayout?.xaxis?.autorange===false)view.x=[...node._fullLayout.xaxis.range];if(node._fullLayout?.yaxis?.autorange===false)view.y=[...node._fullLayout.yaxis.range];if(view.x||view.y)views[node.id]=view});try{sessionStorage.setItem(refreshKey,JSON.stringify({version:versionSelect.value,followLatest:versionSelect.value===names.at(-1),xaxis:xaxis.value,smooth:smooth.checked,ema:ema.value,scroll:[scrollX,scrollY],views}))}catch{}}
function restoreDashboardState(){if(saved?.version===versionSelect.value&&saved.xaxis===xaxis.value)for(const [id,view] of Object.entries(saved.views||{})){const update={};if(view.x)update['xaxis.range']=view.x;if(view.y)update['yaxis.range']=view.y;if(Object.keys(update).length)Plotly.relayout(id,update)}if(saved?.scroll)scrollTo(...saved.scroll)}
function showVersion(){const run=versions[versionSelect.value],reports=run.reports;updateSteps(reports,run);const timed=reports.find(row=>Number.isFinite(Number(row._written))&&Number.isFinite(Number(row.metrics.seconds)));timeOrigin=timed?timed._written-timed.metrics.seconds:0;document.querySelector('#title').textContent=`Spirefysh V${run.version} · ${run.run}`;plot('advantage',series(reports,'mean_advantage'),{history:reports,run});plot('throughput',series(reports,'decisions_per_second'),{tozero:true,history:reports,run});floorPlot(reports,run);stagePlot('ascension',reports,'ascension','#fb7185',run);stagePlot('bonus',reports,'bonus','#f59e0b',run);plot('wins',reports.map(row=>({x:x(row),y:row.metrics.wins/Math.max(1,row.metrics.episodes)})),{range:[0,1],percent:true,history:reports,run});plot('clip',series(reports,'clip_fraction'),{range:[0,1],percent:true,history:reports,run});plot('kl',series(reports,'kl'),{tozero:true,history:reports,run});plot('entropy',series(reports,'entropy'),{tozero:true,history:reports,run});document.querySelector('#promotion').innerHTML=promotionSummary(run.promotions.at(-1))}
function refreshLive(){const selected=versionSelect.value,run=versions[selected];if(!run?.live)return;const script=document.createElement('script');script.src=encodeURI(run.live)+`?${Date.now()}`;script.onload=()=>{script.remove();const live=window.spirefyshLive;if(versionSelect.value!==selected||live?.version!==run.version)return;const report=live.report,last=run.reports.at(-1);if(!last||report.step>last.step||report._written>last._written){run.reports=[...run.reports.filter(row=>row.step!==report.step),report].sort((a,b)=>a.step-b.step);showVersion()}};script.onerror=()=>script.remove();document.head.append(script)}
versionSelect.onchange=showVersion;xaxis.onchange=showVersion;smooth.onchange=showVersion;ema.oninput=showVersion;showVersion();setTimeout(restoreDashboardState,100);window.addEventListener('beforeunload',saveDashboardState);setInterval(refreshLive,15000)</script>"""
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
    global _EVENT_STREAM
    if args.expert_batch is None:
        args.expert_batch = args.batch
    qos = getattr(ctypes.CDLL(None), "pthread_set_qos_class_self_np", None)
    if qos is not None:
        qos(0x21, 0)
    torch.set_num_threads(args.torch_threads)
    torch.manual_seed(args.seed); np.random.seed(args.seed)
    target = device()
    if not 0 <= args.start_stage < len(STAGES):
        raise ValueError("invalid start stage")
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
    if args.sampler_timeout <= 0 or args.sampler_restarts < 0:
        raise ValueError("invalid sampler watchdog")
    if (args.segment_steps < 0 or args.winning_capacity < 0 or args.priority_decay <= 0
            or min(args.critic_consistency_weight, args.search_consistency_weight) < 0
            or min(args.critic_consistency_batch, args.search_consistency_batch) < 1):
        raise ValueError("invalid replay setting")
    if min(args.head_learning_rate_multiplier, args.critic_learning_rate_multiplier) <= 0:
        raise ValueError("invalid head learning-rate multiplier")
    if (args.gae_lambda != 1 or args.segment_steps or not 0 <= args.critic_lambda <= 1
            or not 0 <= args.critic_balance_decay < 1
            or args.blended_critic or args.critic_consistency_weight
            or args.search_consistency_weight and not args.critic_only):
        raise ValueError("invalid categorical critic settings")
    if args.entropy_weight is not None and args.entropy_weight < 0:
        raise ValueError("invalid entropy weight")
    if (not 0 <= args.mcts_fraction <= 1
            or min(args.mcts_simulations, args.mcts_boss_simulations, args.mcts_turns,
                   args.expert_max_lag) < 0
            or min(args.mcts_max_depth, args.mcts_batch_size, args.mcts_min_visits,
                   args.mcts_max_targets, args.expert_batch, args.expert_capacity) < 1
            or min(args.mcts_prior_temperature, args.mcts_q_temperature) <= 0
            or args.mcts_exploration < 0 or args.expert_weight < 0
            or not math.isfinite(args.mcts_timeout) or args.mcts_timeout < 0):
        raise ValueError("invalid MCTS settings")
    if args.envs % args.samplers:
        raise ValueError("environments must be divisible by samplers")
    if args.hours <= 0 and args.decisions <= 0:
        raise ValueError("set --hours or --decisions")
    if args.critic_only and not args.checkpoint:
        raise ValueError("critic-only training requires --checkpoint")
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
        requested_pooling = {
            name: getattr(args, name + "_pooling") for name in POOLING_DEFAULTS
            if getattr(args, name + "_pooling") is not None
        }
        if any(model.pooling[name] != mode for name, mode in requested_pooling.items()):
            raise ValueError("checkpoint pooling configuration cannot be changed")
        args.width, args.layers, args.heads, args.feedforward = loaded
    else:
        config = tuple(value if value is not None else default for value, default in zip(
            (args.width, args.layers, args.heads, args.feedforward), (128, 4, 8, 384)
        ))
        args.width, args.layers, args.heads, args.feedforward = config
        pooling = {name: getattr(args, name + "_pooling") or default
                   for name, default in POOLING_DEFAULTS.items()}
        model = Agent(layout, *config, pooling=pooling).to(target)
    fused_optimizer = target.type != "cpu"
    parameter_groups, group_indices = optimizer_groups(
        model, args.head_learning_rate_multiplier, args.critic_learning_rate_multiplier,
    )
    if args.critic_only:
        model.requires_grad_(False); model.critic.requires_grad_(True)
        parameter_groups = [{"params": list(model.critic.parameters()), "lr_scale": 1.}]
    optimizer = torch.optim.Adam(
        parameter_groups, lr=args.learning_rate, eps=1e-5, fused=fused_optimizer
    )
    optimizer_restored = bool(
        not args.critic_only and source and source["_optimizer_compatible"]
        and repartition_optimizer(source["optimizer"], group_indices)
    )
    if optimizer_restored:
        optimizer.load_state_dict(source["optimizer"])
        torch.set_rng_state(source["torch_rng"].cpu())
    scales = (args.critic_learning_rate_multiplier,) if args.critic_only else (
        1., args.head_learning_rate_multiplier, args.critic_learning_rate_multiplier,
    )
    for group, scale in zip(optimizer.param_groups, scales):
        group["lr"] = args.learning_rate * scale
        group["lr_scale"] = scale
        group["fused"] = fused_optimizer
        group["foreach"] = None
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
    configure_logging(output, "learner", args.log_level)
    _EVENT_STREAM = (output / "events.jsonl").open("a", buffering=1)
    training = {key: value for key, value in vars(args).items() if key != "command"}
    emit_event({
        "time": time.time(), "event": "start", "pid": os.getpid(),
        "model_version": MODEL_VERSION, "checkpoint": args.checkpoint, "training": training,
    })
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
        "entropy": f"fixed {args.entropy_weight}" if args.entropy_weight is not None
                   else "(start -> end) * max(0.35, 0.8^stage)",
    }
    manifest.setdefault("sessions", []).append({"step": source["decisions"] if source else 0, "training": training})
    atomic_json(output / "run.json", manifest)
    reports_dir = output / "reports"; reports_dir.mkdir(exist_ok=continuing)
    checkpoints_dir = output / "checkpoints"; checkpoints_dir.mkdir(exist_ok=continuing)
    development_dir = output / "development"; development_dir.mkdir(exist_ok=continuing)
    champions_dir = output / "stage-champions"; champions_dir.mkdir(exist_ok=continuing)
    entries_dir = output / "stage-entries"; entries_dir.mkdir(exist_ok=continuing)
    promotions_dir = output / "promotions"; promotions_dir.mkdir(exist_ok=continuing)
    stage = source["stage"] if source else args.start_stage
    reservoir = WinningReservoir(args.winning_capacity, args.envs)
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
    critic_balance = CriticBalance(
        args.critic_balance_decay,
        source.get("critic_balance")
        if source and source.get("_source_model_version") == MODEL_VERSION else None,
    )
    progress_active = stage <= 6
    sampler_session = source.get("sampler_session", source.get("sampler_index", 0)) if source else 0
    promotion_index = source.get("promotion_index", 0) if source else 0
    if not 0 <= stage < len(STAGES):
        raise ValueError("invalid checkpoint stage")
    if args.decisions and args.decisions < decisions:
        raise ValueError("--decisions precedes the checkpoint")
    if args.promote_now and (not source or args.decisions != decisions):
        raise ValueError("--promote-now requires an unchanged checkpoint decision target")
    initial = None
    initial_digest = None
    if not source and not continuing:
        initial = output / "initial.pt"
        initial_digest = save_checkpoint(
            initial, model, optimizer, manifest, stage, 0, sampler_session,
            promotion_index, progress_active, reservoir, auxiliary_decisions, stage_decisions,
            critic_balance,
        )
        atomic_json(output / "initial.json", {
            "step": 0, "checkpoint": initial.name, "sha256": initial_digest,
        })
    latest = output / "latest.pt"
    def save(path, step, replace):
        return save_checkpoint(
            path, model, optimizer, manifest, stage, step, sampler_session,
            promotion_index, progress_active, reservoir, auxiliary_decisions + step - decisions,
            stage_decisions + step - decisions, critic_balance, replace,
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
        emit_event({"time": time.time(), "event": "promotion", **promotion})

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
            emit_event({
                "time": time.time(), "event": "checkpoint", "step": step,
                "stage": stage, "path": str(checkpoint), "sha256": immutable_digest,
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
            live = row | {"_written": (output / "live.json").stat().st_mtime}
            atomic_live(output / "live.js", {"version": MODEL_VERSION, "report": live})
            keep = {last_checkpoint[1]} if last_checkpoint else set()
            keep.update(output / row["checkpoint"] for row in stage_bests.values())
            if best.get("checkpoint"):
                keep.add(output / best["checkpoint"])
            for checkpoint in checkpoints_dir.glob("*.pt"):
                if checkpoint not in keep:
                    checkpoint.unlink()
            emit_event({"time": time.time(), "event": "report", **row})
        model.train()
        training = train_stream(
            model, optimizer, args, sampler_session, stage, target, deadline, budget,
            base, auxiliary_decisions, stage_decisions, run_started, reservoir, progress_active,
            save_report, save_step, manifest["fingerprint"], critic_balance,
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
    emit_event({
        "time": time.time(), "event": "complete", "decisions": decisions,
        "stage": stage, "training_seconds": training_seconds,
        "promotion_seconds": promotion_seconds,
    })
    dashboard(output.parent)
    _EVENT_STREAM.close(); _EVENT_STREAM = None
    shutdown_logging()


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
        )), pooling=config["pooling"]).to(target)
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError("invalid checkpoint architecture") from error
    if config != architecture(model):
        raise ValueError("incompatible model architecture")
    state = checkpoint["model"]
    missing, unexpected = model.load_state_dict(state, strict=True)
    if missing or unexpected:
        raise ValueError("incompatible model parameters")
    checkpoint["_optimizer_compatible"] = optimizer_compatible(
        checkpoint["optimizer"], len(tuple(model.parameters()))
    )
    checkpoint["_source_model_version"] = version
    checkpoint["model_version"] = MODEL_VERSION
    checkpoint["architecture"] = architecture(model)
    return model, checkpoint


def diagnose_search(args):
    model, checkpoint = load(args.checkpoint, torch.device("cpu")); model.eval()
    ascension, bonus = STAGES[checkpoint["stage"]]
    env = sts2_sim.Batch(args.envs, args.seed, None, ascension=ascension)
    env.set_training_bonus(bonus)
    env.load_policy(export_value_model(None, model, env.fingerprint(), 1, 0, True))
    previous = [-1] * args.envs
    roots = []
    for step in range(args.max_steps):
        stats = env.stats(); seeds = env.seeds()
        for index, row in enumerate(stats):
            turn = row[5] if row[4] == 1 else -1
            fresh = turn >= 0 and turn != previous[index]
            previous[index] = turn
            ticket = (seeds[index] * 0x9E3779B1 + row[1] * 131 + turn * 17) & 0xffffffff
            if not fresh or ticket % args.sample_stride or len(roots) >= args.roots:
                continue
            started = time.monotonic()
            try:
                result = json.loads(env.search_diagnostics(
                    index, args.simulations, 1, args.max_depth, args.batch_size,
                    args.prior_temperature, args.exploration, args.policy_temperature,
                    args.exact_samples, args.max_exact_states, args.seed + len(roots),
                    args.progress_value, args.q_temperature,
                ))
            except ValueError as error:
                print(json.dumps({"skipped": {"seed": seeds[index], "floor": row[1], "turn": turn},
                                  "error": str(error)}), flush=True)
                continue
            result["seconds"] = time.monotonic() - started
            roots.append(result)
            print(json.dumps({"root": result["root"], "mcts": result["mcts"],
                              "exact": result["exact"], "seconds": result["seconds"]}), flush=True)
        if len(roots) >= args.roots:
            break
        result = env.policy(args.policy_temperature, True, True)
        done = np.flatnonzero(result[8]).tolist()
        if done:
            env.reset(done, 0)
            for index in done:
                previous[index] = -1
    if not roots:
        raise RuntimeError("no turn-start roots sampled")
    output = Path(args.output); output.parent.mkdir(parents=True, exist_ok=True)
    raw = output.with_suffix(".json")
    raw.write_text(json.dumps({"checkpoint": args.checkpoint, "roots": roots}, indent=2))
    nodes = [node | {"root": root_index, **root["root"]}
             for root_index, root in enumerate(roots) for node in root["nodes"]]
    pairs = []; total_pairs = 0; rng = np.random.default_rng(args.seed)
    for root_index, root in enumerate(roots):
        leaves = root["leaves"]
        for left in range(len(leaves)):
            for right in range(left + 1, len(leaves)):
                total_pairs += 1
                high, low = (leaves[left], leaves[right]) \
                    if leaves[left]["value"] >= leaves[right]["value"] \
                    else (leaves[right], leaves[left])
                pair = {
                    "root": root_index,
                    "delta_value": high["value"] - low["value"],
                    "delta_player_hp": high["player_hp"] - low["player_hp"],
                    "delta_enemy_hp": high["enemy_hp"] - low["enemy_hp"],
                    "weight": high["weight"] * low["weight"],
                }
                if len(pairs) < args.max_pairs:
                    pairs.append(pair)
                else:
                    replace = int(rng.integers(total_pairs))
                    if replace < args.max_pairs:
                        pairs[replace] = pair
    leaf_fits = []
    for root_index, root in enumerate(roots):
        value = np.asarray([leaf["value"] for leaf in root["leaves"]])
        features = np.asarray([[1, leaf["player_hp"], leaf["enemy_hp"]] for leaf in root["leaves"]])
        coefficient = np.linalg.lstsq(features, value, rcond=None)[0]
        residual = np.square(value - features @ coefficient).sum()
        total = np.square(value - value.mean()).sum()
        leaf_fits.append({"root": root_index, "value_per_player_hp": float(coefficient[1]),
                          "value_per_enemy_hp": float(coefficient[2]),
                          "r2": float(1 - residual / total) if total else None})
    exact = np.asarray([node["exact"] for node in nodes]); approximate = np.asarray([node["approximate"] for node in nodes])
    errors = np.abs(exact - approximate); visits = np.asarray([node["visits"] for node in nodes])
    actions = [action | {"root": root_index, "node": node["node"], "depth": node["depth"]}
               for root_index, root in enumerate(roots) for node in root["nodes"]
               for action in node["actions"] if action["approximate"] is not None]
    action_exact = np.asarray([action["exact"] for action in actions])
    action_approximate = np.asarray([action["approximate"] for action in actions])
    action_errors = np.abs(action_exact - action_approximate)
    gaps = [root["root_policy"] for root in roots]
    summary = {
        "objective": "progress" if args.progress_value else "win",
        "roots": len(roots), "nodes": len(nodes), "leaf_pairs": total_pairs,
        "plotted_leaf_pairs": len(pairs), "mae": float(errors.mean()),
        "rmse": float(np.sqrt(np.mean((exact - approximate) ** 2))),
        "correlation": float(np.corrcoef(exact, approximate)[0, 1]) if len(nodes) > 1 else None,
        "visit_error_correlation": float(np.corrcoef(np.log1p(visits), errors)[0, 1]) if len(nodes) > 1 else None,
        "policy_expected_gap_mean": float(np.mean([row["policy_expected_gap"] for row in gaps])),
        "policy_greedy_gap_mean": float(np.mean([row["policy_greedy_gap"] for row in gaps])),
        "search_gap_mean": float(np.mean([row["search_gap"] for row in gaps])),
        "search_q_gap_mean": float(np.mean([row["search_q_gap"] for row in gaps])),
        "q_actions": len(actions), "q_mae": float(action_errors.mean()),
        "q_rmse": float(np.sqrt(np.mean((action_exact - action_approximate) ** 2))),
        "q_correlation": float(np.corrcoef(action_exact, action_approximate)[0, 1]),
        "q_target_gap_mean": float(np.mean([row["q_target_gap"] for row in gaps])),
        "q_target_entropy_mean": float(np.mean([row["q_target_entropy"] for row in gaps])),
        "leaf_value_linear_fits": leaf_fits,
    }
    if args.progress_value:
        summary |= {"mae_floors": summary["mae"] * 52,
                    "rmse_floors": summary["rmse"] * 52,
                    "policy_expected_gap_floors": summary["policy_expected_gap_mean"] * 52,
                    "search_gap_floors": summary["search_gap_mean"] * 52,
                    "search_q_gap_floors": summary["search_q_gap_mean"] * 52,
                    "q_mae_floors": summary["q_mae"] * 52,
                    "q_rmse_floors": summary["q_rmse"] * 52,
                    "q_target_gap_floors": summary["q_target_gap_mean"] * 52}
    data = json.dumps({"nodes": nodes, "pairs": pairs, "roots": roots, "summary": summary}).replace("</", "<\\/")
    html = output.with_suffix(".html")
    html.write_text("""<!doctype html><meta charset=utf-8><title>Search diagnostics</title>
<script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script><style>body{font:14px system-ui;margin:24px;background:#101319;color:#e8ecf2}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(520px,1fr));gap:18px}.plot{height:560px;background:#171d28;border:1px solid #303a49;border-radius:10px}pre{white-space:pre-wrap}</style>
<h1>Exact vs approximate expectimax</h1><p>Crosses have zero outgoing visits: MCTS expanded and evaluated the node, backed its value up through the incoming edge, but never traversed an action from that node. Their approximate value is therefore the raw value-model estimate.</p><pre id=summary></pre><div id=values class=grid></div><h1>Exact versus sampled action Q</h1><div id=actions class=grid></div><h1>Value geometry across exhaustive leaves</h1><p>Signed HP differences are better-valued leaf minus worse-valued leaf. Negative enemy HP therefore means the value model prefers a leaf with less enemy life remaining.</p><div id=leaves class=grid></div><script>const data=""" + data + r""",config={responsive:true,displaylogo:false},base={template:'plotly_dark',paper_bgcolor:'#171d28',plot_bgcolor:'#171d28',margin:{l:70,r:30,t:55,b:60}};
summary.textContent=JSON.stringify(data.summary,null,2);const progress=data.summary.objective==='progress',scale=progress?53:1,valueName=progress?'predicted terminal category':'win probability';function panel(parent,id){const div=document.createElement('div');div.id=id;div.className='plot';document.querySelector(parent).append(div);return div}data.roots.forEach((root,index)=>{const label=`Root ${index} · character ${root.root.character} · floor ${root.root.floor} turn ${root.root.turn}`,n=data.nodes.filter(row=>row.root===index),x=n.map(row=>row.exact*scale),y=n.map(row=>row.approximate*scale),lo=Math.min(...x,...y),hi=Math.max(...x,...y),valuePanel=panel('#values',`values-${index}`);Plotly.newPlot(valuePanel,[{x,y,mode:'markers',marker:{size:n.map(row=>row.visits?5+3*Math.log1p(row.visits):8),symbol:n.map(row=>row.visits?'circle':'x'),color:n.map(row=>Math.log1p(row.visits)),colorscale:'Viridis',showscale:true,colorbar:{title:'log(1+visits)'}},customdata:n.map(row=>[row.depth,row.visits,row.absolute_error*scale,row.rng]),hovertemplate:'depth %{customdata[0]} · visits %{customdata[1]}<br>exact %{x:.6f}<br>approximate %{y:.6f}<br>|error| %{customdata[2]:.6f}<br>RNG %{customdata[3]}<extra></extra>'},{x:[lo,hi],y:[lo,hi],mode:'lines',line:{dash:'dash',color:'#ddd'},name:'identity'}],{...base,title:label,xaxis:{title:`Repeated-exhaustive ${valueName}`,gridcolor:'#303a49'},yaxis:{title:`MCTS ${valueName}`,gridcolor:'#303a49'},showlegend:false},config);const q=root.nodes.flatMap(node=>node.actions.map(action=>({...action,depth:node.depth}))).filter(row=>row.approximate!==null),qx=q.map(row=>row.exact*scale),qy=q.map(row=>row.approximate*scale),qlo=Math.min(...qx,...qy),qhi=Math.max(...qx,...qy),actionPanel=panel('#actions',`actions-${index}`);Plotly.newPlot(actionPanel,[{x:qx,y:qy,mode:'markers',marker:{size:q.map(row=>5+2.5*Math.log1p(row.visits)),color:q.map(row=>Math.log1p(row.visits)),colorscale:'Viridis',showscale:true,colorbar:{title:'log(1+visits)'},opacity:.65},customdata:q.map(row=>[row.action,row.depth,row.visits,row.target]),hovertemplate:'%{customdata[0]}<br>depth %{customdata[1]} · visits %{customdata[2]}<br>exact Q %{x:.6f}<br>sampled Q %{y:.6f}<br>target %{customdata[3]:.4f}<extra></extra>'},{x:[qlo,qhi],y:[qlo,qhi],mode:'lines',line:{dash:'dash',color:'#ddd'},name:'identity'}],{...base,title:label,xaxis:{title:`Repeated-exhaustive Q (${valueName})`,gridcolor:'#303a49'},yaxis:{title:`Sampled expectimax Q (${valueName})`,gridcolor:'#303a49'},showlegend:false},config);const p=data.pairs.filter(row=>row.root===index),z=p.map(row=>row.delta_value*scale),leafPanel=panel('#leaves',`leaves-${index}`);Plotly.newPlot(leafPanel,[{type:'scatter3d',mode:'markers',x:p.map(row=>row.delta_player_hp),y:p.map(row=>row.delta_enemy_hp),z,marker:{size:3,opacity:.35,color:z,colorscale:'Plasma',showscale:true,colorbar:{title:`Δ ${valueName}`}},customdata:p.map(row=>row.weight),hovertemplate:`Δ player HP %{x}<br>Δ enemy HP %{y}<br>Δ ${valueName} %{z:.6f}<br>particle-pair weight %{customdata}<extra></extra>`}],{...base,title:label,scene:{xaxis:{title:'Δ player HP'},yaxis:{title:'Δ enemy remaining HP'},zaxis:{title:`Δ ${valueName}`}}},config)})</script>""")
    print(json.dumps(summary, indent=2)); print(html); print(raw)


def compare_search(args):
    runs = []
    for item in args.input:
        label, path = item.split("=", 1)
        data = json.loads(Path(path).read_text())
        nodes = [node for root in data["roots"] for node in root["nodes"]]
        exact = np.asarray([node["exact"] for node in nodes])
        approximate = np.asarray([node["approximate"] for node in nodes])
        actions = [action for root in data["roots"] for node in root["nodes"]
                   for action in node["actions"] if action["approximate"] is not None]
        action_exact = np.asarray([action["exact"] for action in actions])
        action_approximate = np.asarray([action["approximate"] for action in actions])
        summary = {
            "nodes": len(nodes),
            "mae": float(np.abs(exact - approximate).mean()),
            "correlation": float(np.corrcoef(exact, approximate)[0, 1]),
            "q_actions": len(actions),
            "q_mae": float(np.abs(action_exact - action_approximate).mean()),
            "q_correlation": float(np.corrcoef(action_exact, action_approximate)[0, 1]),
            "search_gap_mean": float(np.mean([root["root_policy"]["search_gap"] for root in data["roots"]])),
            "search_q_gap_mean": float(np.mean([root["root_policy"]["search_q_gap"] for root in data["roots"]])),
            "q_target_gap_mean": float(np.mean([root["root_policy"]["q_target_gap"] for root in data["roots"]])),
        }
        runs.append({"temperature": label, "objective": data["roots"][0]["settings"]["objective"], "summary": summary,
                     "roots": [{"root": root["root"], "nodes": root["nodes"]}
                               for root in data["roots"]]})
    if not runs or any(len(run["roots"]) != len(runs[0]["roots"]) or
                       run["objective"] != runs[0]["objective"] for run in runs):
        raise ValueError("search diagnostics have incompatible roots")
    data = json.dumps(runs).replace("</", "<\\/")
    output = Path(args.output); output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("""<!doctype html><meta charset=utf-8><title>MCTS prior-temperature comparison</title>
<script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script><style>body{font:14px system-ui;margin:24px;background:#101319;color:#e8ecf2}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(560px,1fr));gap:18px}.plot{height:560px;background:#171d28;border:1px solid #303a49;border-radius:10px}table{border-collapse:collapse;margin-bottom:24px}th,td{padding:7px 12px;border-bottom:1px solid #303a49;text-align:right}th:first-child,td:first-child{text-align:left}</style>
<h1>MCTS prior-temperature comparison</h1><table id=summary></table><h1>Node values</h1><div id=plots class=grid></div><h1>Action Q values</h1><div id=qplots class=grid></div><script>const runs=""" + data + r""",scale=runs[0].objective==='progress'?53:1,unit=runs[0].objective==='progress'?'terminal category':'win probability',colors=['#60a5fa','#f59e0b','#f472b6','#34d399'],config={responsive:true,displaylogo:false},base={template:'plotly_dark',paper_bgcolor:'#171d28',plot_bgcolor:'#171d28',margin:{l:70,r:25,t:55,b:60}};summary.innerHTML=`<tr><th>Prior temperature</th><th>Nodes</th><th>Node MAE (${unit})</th><th>Node corr.</th><th>Q MAE (${unit})</th><th>Q corr.</th><th>Visit gap</th><th>Q argmax gap</th><th>Q target gap</th></tr>`+runs.map(run=>`<tr><td>${run.temperature}</td><td>${run.summary.nodes}</td><td>${Number(run.summary.mae*scale).toPrecision(4)}</td><td>${Number(run.summary.correlation).toFixed(4)}</td><td>${Number(run.summary.q_mae*scale).toPrecision(4)}</td><td>${Number(run.summary.q_correlation).toFixed(4)}</td><td>${Number(run.summary.search_gap_mean*scale).toPrecision(4)}</td><td>${Number(run.summary.search_q_gap_mean*scale).toPrecision(4)}</td><td>${Number(run.summary.q_target_gap_mean*scale).toPrecision(4)}</td></tr>`).join('');for(let root=0;root<runs[0].roots.length;root++){const div=document.createElement('div');div.className='plot';plots.append(div);const all=runs.flatMap(run=>run.roots[root].nodes),lo=Math.min(...all.flatMap(row=>[row.exact,row.approximate]))*scale,hi=Math.max(...all.flatMap(row=>[row.exact,row.approximate]))*scale,info=runs[0].roots[root].root,traces=runs.map((run,index)=>{const rows=run.roots[root].nodes;return{x:rows.map(row=>row.exact*scale),y:rows.map(row=>row.approximate*scale),mode:'markers',name:`temperature ${run.temperature}`,marker:{color:colors[index%colors.length],size:rows.map(row=>row.visits?5+2.5*Math.log1p(row.visits):7),symbol:rows.map(row=>row.visits?'circle':'x'),opacity:.6},customdata:rows.map(row=>[row.depth,row.visits,row.absolute_error*scale]),hovertemplate:`temperature ${run.temperature}<br>depth %{customdata[0]} · visits %{customdata[1]}<br>exact %{x:.6f}<br>approximate %{y:.6f}<br>|error| %{customdata[2]:.6f}<extra></extra>`}});traces.push({x:[lo,hi],y:[lo,hi],mode:'lines',name:'identity',line:{color:'#ddd',dash:'dash'}});Plotly.newPlot(div,traces,{...base,title:`Root ${root} · character ${info.character} · floor ${info.floor} turn ${info.turn}`,xaxis:{title:`Repeated-exhaustive ${unit}`,gridcolor:'#303a49',range:[lo,hi]},yaxis:{title:`MCTS ${unit}`,gridcolor:'#303a49',range:[lo,hi]},legend:{orientation:'h'}},config);const qdiv=document.createElement('div');qdiv.className='plot';qplots.append(qdiv);const qall=runs.flatMap(run=>run.roots[root].nodes.flatMap(node=>node.actions.map(action=>({...action,depth:node.depth}))).filter(row=>row.approximate!==null)),qlo=Math.min(...qall.flatMap(row=>[row.exact,row.approximate]))*scale,qhi=Math.max(...qall.flatMap(row=>[row.exact,row.approximate]))*scale,qtraces=runs.map((run,index)=>{const rows=run.roots[root].nodes.flatMap(node=>node.actions.map(action=>({...action,depth:node.depth}))).filter(row=>row.approximate!==null);return{x:rows.map(row=>row.exact*scale),y:rows.map(row=>row.approximate*scale),mode:'markers',name:`temperature ${run.temperature}`,marker:{color:colors[index%colors.length],size:rows.map(row=>5+2.5*Math.log1p(row.visits)),opacity:.55},customdata:rows.map(row=>[row.action,row.depth,row.visits,row.target]),hovertemplate:`temperature ${run.temperature}<br>%{customdata[0]}<br>depth %{customdata[1]} · visits %{customdata[2]}<br>exact Q %{x:.6f}<br>sampled Q %{y:.6f}<br>target %{customdata[3]:.4f}<extra></extra>`}});qtraces.push({x:[qlo,qhi],y:[qlo,qhi],mode:'lines',name:'identity',line:{color:'#ddd',dash:'dash'}});Plotly.newPlot(qdiv,qtraces,{...base,title:`Root ${root} · character ${info.character} · floor ${info.floor} turn ${info.turn}`,xaxis:{title:`Repeated-exhaustive Q (${unit})`,gridcolor:'#303a49',range:[qlo,qhi]},yaxis:{title:`Sampled expectimax Q (${unit})`,gridcolor:'#303a49',range:[qlo,qhi]},legend:{orientation:'h'}},config)}</script>""")
    print(output)


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
            values.extend(critic_win_logit(model(*inputs[:6])[1]).cpu().tolist())
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
        struct.pack("<10I", model.width, model.layers, model.heads, model.feedforward,
                    len(TOKEN_SPECS), len(SEMANTIC_NAMES), model.concepts.num_embeddings,
                    ACTION_FIELDS[2], ACTION_FIELDS[3], CATEGORIES),
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
    for parameter in model.critic.parameters():
        parameter.requires_grad_(True)
    optimizer = torch.optim.Adam(model.critic.parameters(), lr=args.learning_rate)
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
                critic_win_logit(model(*inputs[:6])[1]), labels,
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
        python_values = (
            critic_win_logit(predict(model, parity_inputs, "fp32")[1]) / temperature + bias
        ).sigmoid().cpu().numpy()
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
    target = torch.device("cpu")
    assert bounded_mcts_timeout(0, 120) == 60
    assert bounded_mcts_timeout(20, 120) == 20
    assert bounded_mcts_timeout(200, 120) == 60
    assert not sampler_wedged(0, None, 200, 120)
    assert not sampler_wedged(0, 195, 200, 120)
    assert sampler_wedged(0, 180, 200, 120)
    env = sts2_sim.Batch(2, 84, 0)
    layout = dict(env.token_layout())
    assert (layout["version"], layout["model_width"], layout["model_layers"],
            layout["model_heads"], layout["model_feedforward"], layout["state_width"],
            layout["action_width"], layout["entity_summaries"]) == (56, 128, 4, 8, 384, 128, 128, 0)
    for name, size in {
        "enemy_position": 33, "power_position": 65, "orb_position": 17,
        "map_floor_position": 65, "deck_origin": 257, "draw_top_position": 257,
        "draw_bottom_position": 257, "relic_position": 257, "wax_position": 257,
        "continuation_position": 257, "crystal_row": 11, "crystal_column": 11,
        "crystal_width": 4, "crystal_height": 4,
    }.items():
        assert layout[name + "_semantic_count"] == size

    for _ in range(3):
        observation = env.observe_tokens()
        legal = np.asarray(observation[3][4])
        choices = [int(np.flatnonzero(row)[0]) for row in legal]
        env.step(choices)
    combat = env.observe_tokens()
    noncombat = sts2_sim.Batch(2, 71, 0).observe_tokens()

    def expected_lengths(observation, pooling):
        domains, action_rows, _positions, _legal = flat_domains(observation)
        batch = len(observation[0])
        counts = np.full(batch, 8, np.int64)
        def rows(domain, predicate=lambda _u: True):
            u, _s, _c, _f, row, scope = domains[DOMAIN[domain]]
            selected = (scope == -1) & predicate(u)
            return u[selected], row[selected]
        def collection(name, domain, predicate, active):
            _u, row = rows(domain, predicate)
            item = np.bincount(row, minlength=batch)
            counts[active] += item[active] if pooling[name] == "global_tokens" else 1
        actor_u, actor_rows = rows("actor")
        combat_rows = np.unique(actor_rows)
        all_rows = np.arange(batch)
        collection("deck", "card", lambda u: u[:, 0] == 0, all_rows)
        collection("relic", "relic", lambda u: u[:, 0] == 0, all_rows)
        collection("potion", "potion", lambda u: u[:, 0] == 0, all_rows)
        for name, zone in (("hand", 1), ("draw", 2), ("discard", 3), ("exhaust", 4)):
            collection(name, "card", lambda u, zone=zone: u[:, 0] == zone, combat_rows)
        collection("orb", "orb", lambda _u: True, combat_rows)
        continuation_u, continuation_rows = rows("continuation")
        for row in range(batch):
            q = len(np.unique(continuation_u[continuation_rows == row, 2]))
            counts[row] += q if pooling["continuation"] == "global_tokens" else 1
        _crystal_u, crystal_rows = rows("crystal")
        counts += np.bincount(crystal_rows, minlength=batch)
        counts += np.bincount(action_rows, minlength=batch)
        for row in combat_rows:
            actors = actor_u[actor_rows == row]
            counts[row] += len(actors)
            for name, enemy in (("friendly_effect", False), ("enemy_effect", True)):
                owners = actors[(actors[:, 1] == 2) == enemy, 0]
                effect_count = 0
                for domain in ("power", "status"):
                    effect_u, effect_rows = rows(domain)
                    effect_count += np.isin(effect_u[effect_rows == row, 0], owners).sum()
                mode = pooling[name]
                counts[row] += 0 if mode.endswith("into_actor") else \
                    effect_count if mode == "global_tokens" else len(owners)
        return counts

    configs = [{}]
    for name in ("relic", "deck", "draw", "exhaust", "discard", "hand", "orb", "potion"):
        configs.extend(({name: mode} for mode in COLLECTION_POOLING))
    for name in ("enemy_effect", "friendly_effect"):
        configs.extend(({name: mode} for mode in EFFECT_POOLING))
    configs.extend(({"continuation": mode} for mode in CONTINUATION_POOLING))
    configs.extend(({"phase": mode} for mode in ("sum", "transformer")))
    configs.extend(({"generation_pool": mode} for mode in ("sum", "transformer")))
    with tempfile.TemporaryDirectory() as parity_directory:
        for number, config in enumerate(configs):
            torch.manual_seed(number)
            model = Agent(layout, pooling=config).eval()
            model.critic.weight.data.normal_(std=.1); model.critic.bias.data.normal_(std=.1)
            for observation in (noncombat, combat):
                inputs = tensors(observation, target, model)
                with torch.no_grad():
                    policy, critic = predict(model, inputs, "fp32")
                assert policy.shape[0] == critic.shape[0] == 2 and critic.shape[1] == CATEGORIES
                assert torch.isfinite(policy).all() and torch.isfinite(critic).all()
                assert np.array_equal(model._sequence_lengths.cpu(),
                                      expected_lengths(observation, model.pooling))
            exported = Path(parity_directory) / f"{number}.bin"
            export_value_model(exported, model, env.fingerprint(), 1, 0)
            rust = np.asarray(env.rust_values(str(exported)))
            python = critic_win_logit(critic).sigmoid().numpy()
            assert np.max(np.abs(python - rust)) <= 1e-5

    model = Agent(layout).eval()
    empty_domains = []
    for values in noncombat[2]:
        values = [np.array(value, copy=True) for value in values]
        values[5][values[4] >= 0] = 0
        empty_domains.append(tuple(values))
    empty_actions = [np.array(value, copy=True) for value in noncombat[3]]
    empty_actions[4].fill(0); empty_actions[5].fill(0)
    no_actions = (*noncombat[:2], tuple(empty_domains), tuple(empty_actions), *noncombat[4:])
    with torch.no_grad():
        policy, critic = predict(model, tensors(no_actions, target, model), "fp32")
    assert policy.shape == empty_actions[5].shape and not policy.any()
    assert critic.shape == (len(noncombat[0]), CATEGORIES)
    assert np.array_equal(model._sequence_lengths.cpu(), expected_lengths(no_actions, model.pooling))

    sequence = torch.randn(1, 3, model.width)
    forward, _ = model.move_gru(sequence)
    backward, _ = model.move_gru(sequence.flip(1))
    assert not torch.allclose(forward[:, -1], backward[:, -1])
    inputs = tensors(combat, target, model)
    with torch.no_grad():
        original = predict(model, inputs, "fp32")
    permutation = np.arange(np.asarray(combat[3][4]).shape[1])[::-1]
    inverse = np.argsort(permutation)
    permuted_domains = []
    for values in combat[2]:
        values = [np.array(value, copy=True) for value in values]
        scoped = values[4] >= 0
        values[4][scoped] = inverse[values[4][scoped]]
        permuted_domains.append(tuple(values))
    permuted_actions = tuple(np.asarray(value)[:, permutation] for value in combat[3])
    permuted_observation = (combat[0], combat[1], tuple(permuted_domains),
                            permuted_actions, combat[4], combat[5])
    with torch.no_grad():
        permuted = predict(model, tensors(permuted_observation, target, model), "fp32")
    assert torch.allclose(original[0], permuted[0][:, inverse], atol=1e-6)
    assert torch.allclose(original[1], permuted[1], atol=1e-6)

    mixed = {
        "relic": "transformer", "deck": "sum", "draw": "transformer",
        "hand": "sum", "discard": "sum", "exhaust": "transformer",
        "orb": "sum", "potion": "transformer", "friendly_effect": "transformer_into_actor",
        "enemy_effect": "sum_token", "continuation": "global_tokens",
        "phase": "sum", "generation_pool": "sum",
    }
    torch.manual_seed(12)
    model = Agent(layout, pooling=mixed).eval()
    model.critic.weight.data.normal_(std=.1); model.critic.bias.data.normal_(std=.1)
    inputs = tensors(combat, target, model)
    with torch.no_grad():
        python = (critic_win_logit(predict(model, inputs, "fp32")[1]) / .83 - .17).sigmoid().numpy()
    with tempfile.TemporaryDirectory() as directory:
        exported = Path(directory) / "value.bin"
        export_value_model(exported, model, env.fingerprint(), .83, -.17)
        rust = np.asarray(env.rust_values(str(exported)))
        invalid_binary = bytearray(exported.read_bytes())
        cap_offset = 8 + 16 + 40 + 64 * 4 + len(SEMANTIC_NAMES) * 4
        struct.pack_into("<I", invalid_binary, cap_offset,
                         struct.unpack_from("<I", invalid_binary, cap_offset)[0] + 1)
        invalid_path = Path(directory) / "invalid.bin"
        invalid_path.write_bytes(invalid_binary)
        try:
            env.rust_values(str(invalid_path))
            raise AssertionError("accepted incompatible binary architecture")
        except (OSError, ValueError):
            pass
        env.load_policy(export_value_model(None, model, env.fingerprint(), 1, 0, actor=True))
        assert len(env.policy(sample=False, advance=False)[0]) == 2
        checkpoint = Path(directory) / "checkpoint.pt"
        torch.save({
            "schema": 1, "model_version": MODEL_VERSION, "feature_version": FEATURE_VERSION,
            "fingerprint": env.fingerprint(), "layout": layout, "architecture": architecture(model),
            "model": model.state_dict(), "optimizer": {"state": {}, "param_groups": []},
        }, checkpoint)
        restored, _ = load(checkpoint, target)
        assert restored.pooling == model.pooling
        incompatible = torch.load(checkpoint, weights_only=False)
        incompatible["architecture"]["position_caps"]["enemy"] = 31
        torch.save(incompatible, checkpoint)
        try:
            load(checkpoint, target)
            raise AssertionError("accepted incompatible checkpoint architecture")
        except ValueError:
            pass
        incompatible = torch.load(checkpoint, weights_only=False)
        incompatible["architecture"]["position_caps"]["enemy"] = 32
        incompatible["model_version"] = 71
        torch.save(incompatible, checkpoint)
        try:
            load(checkpoint, target)
            raise AssertionError("accepted pre-v71 checkpoint")
        except ValueError:
            pass
    assert np.max(np.abs(python - rust)) <= 1e-5
    train_model = Agent(layout, pooling=mixed).train()
    inputs = tensors(combat, target, train_model)
    policy, critic = predict(train_model, inputs, "fp32")
    (policy.sum() + critic.sum()).backward()
    assert all(parameter.grad is None or parameter.grad.isfinite().all()
               for parameter in train_model.parameters())
    accelerator = device()
    if accelerator.type in ("mps", "cuda"):
        accelerated = Agent(layout, pooling=mixed).to(accelerator).train()
        inputs = tensors(combat, accelerator, accelerated)
        policy, critic = predict(accelerated, inputs, "bf16")
        (policy.sum() + critic.sum()).backward()
        assert all(parameter.grad is None or parameter.grad.isfinite().all()
                   for parameter in accelerated.parameters())
    print("probe ok")


def parser():
    root = argparse.ArgumentParser()
    commands = root.add_subparsers(dest="command", required=True)
    run = commands.add_parser("train")
    run.add_argument("--output", default="target/v71")
    run.add_argument("--checkpoint")
    run.add_argument("--width", type=int)
    run.add_argument("--layers", type=int)
    run.add_argument("--heads", type=int)
    run.add_argument("--feedforward", type=int)
    for name in ("relic", "deck", "draw", "exhaust", "discard", "hand", "orb", "potion"):
        run.add_argument(f"--{name}-pooling", choices=COLLECTION_POOLING,
                         default=None)
    for name in ("enemy-effect", "friendly-effect"):
        run.add_argument(f"--{name}-pooling", choices=EFFECT_POOLING,
                         default=None)
    run.add_argument("--continuation-pooling", choices=CONTINUATION_POOLING,
                     default=None)
    run.add_argument("--phase-pooling", choices=("sum", "transformer"),
                     default=None)
    run.add_argument("--generation-pool-pooling", choices=("sum", "transformer"),
                     default=None)
    run.add_argument("--precision", choices=PRECISIONS, default="bf16")
    run.add_argument("--start-stage", type=int, default=0)
    run.add_argument("--hours", type=float, default=0)
    run.add_argument("--decisions", type=int, default=0)
    run.add_argument("--envs", type=int, default=512)
    run.add_argument("--samplers", type=int, default=4)
    run.add_argument("--torch-threads", type=int, default=2)
    run.add_argument("--sampler-threads", type=int, default=1)
    run.add_argument("--sampler-backend", choices=("process", "thread"), default="process")
    run.add_argument("--log-level", choices=("DEBUG", "INFO", "WARNING", "ERROR"), default="INFO")
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
    run.add_argument("--mcts-fraction", type=float, default=0)
    run.add_argument("--mcts-simulations", type=int, default=0)
    run.add_argument("--mcts-boss-simulations", type=int, default=0)
    run.add_argument("--mcts-turns", type=int, default=1)
    run.add_argument("--mcts-max-depth", type=int, default=64)
    run.add_argument("--mcts-batch-size", type=int, default=256)
    run.add_argument("--mcts-min-visits", type=int, default=16)
    run.add_argument("--mcts-max-targets", type=int, default=64)
    run.add_argument("--mcts-prior-temperature", type=float, default=1)
    run.add_argument("--mcts-q-temperature", type=float, default=.002)
    run.add_argument("--mcts-exploration", type=float, default=1.5)
    run.add_argument("--mcts-heuristic", action="store_true")
    run.add_argument("--mcts-timeout", type=float, default=0)
    run.add_argument("--expert-weight", type=float, default=.002)
    run.add_argument("--expert-max-lag", type=int, default=4)
    run.add_argument("--expert-batch", type=int)
    run.add_argument("--expert-capacity", type=int, default=131_072)
    run.add_argument("--batch", type=int, default=1024)
    run.add_argument("--learning-rate", type=float, default=3e-4)
    run.add_argument("--head-learning-rate-multiplier", type=float, default=1)
    run.add_argument("--critic-learning-rate-multiplier", type=float, default=1)
    run.add_argument("--critic-only", action="store_true")
    run.add_argument("--gae-lambda", type=float, default=1.0)
    run.add_argument("--critic-lambda", type=float, default=1.0)
    run.add_argument("--critic-balance-decay", type=float, default=.99)
    run.add_argument("--progress-gamma", type=float, default=1.0)
    run.add_argument("--critic-consistency-weight", type=float, default=0)
    run.add_argument("--critic-consistency-batch", type=int, default=1024)
    run.add_argument("--search-consistency-weight", type=float, default=0)
    run.add_argument("--search-consistency-batch", type=int, default=256)
    run.add_argument("--blended-critic", action="store_true")
    run.add_argument("--critic-win-ema-decay", type=float, default=.9)
    run.add_argument("--critic-blend-power", type=float, default=.4)
    run.add_argument("--progress-decisions", type=int, default=5_000_000)
    run.add_argument("--progress-beta", type=float)
    run.add_argument("--clip", type=float, default=0.2)
    run.add_argument("--value-weight", type=float, default=0.5)
    run.add_argument("--winning-loss-weight", type=float, default=0.1)
    run.add_argument("--winning-capacity", type=int, default=WINNING_CAPACITY)
    run.add_argument("--priority-decay", type=float, default=3)
    run.add_argument("--character-balanced", action="store_true")
    run.add_argument("--entropy-start", type=float, default=0.01)
    run.add_argument("--entropy-end", type=float, default=0.001)
    run.add_argument("--entropy-weight", type=float)
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
    evaluate_parser.add_argument("--output")
    evaluate_parser.add_argument("--ascension", type=int, default=0)
    evaluate_parser.add_argument("--bonus", type=int, default=24)
    evaluate_parser.add_argument("--validation-seed", type=int, default=3_500_000_000)
    evaluate_parser.add_argument("--runs", type=int, default=128)
    evaluate_parser.add_argument("--evaluation-batch", type=int, default=32)
    evaluate_parser.add_argument("--max-steps", type=int, default=2048)
    evaluate_parser.add_argument("--max-combat-steps", type=int, default=512)
    evaluate_parser.add_argument("--precision", choices=PRECISIONS, default="fp32")
    search_evaluation = commands.add_parser("evaluate-search-policies")
    search_evaluation.add_argument("checkpoint")
    search_evaluation.add_argument("--output", default="target/search-policy-evaluation.json")
    search_evaluation.add_argument(
        "--modes", nargs="+", choices=(
            "policy", "sampled", "exact", "heuristic", "combat", "combat-heuristic",
        ),
        default=("policy", "sampled", "exact"),
    )
    search_evaluation.add_argument("--runs", type=int, default=16)
    search_evaluation.add_argument("--batch", type=int, default=16)
    search_evaluation.add_argument("--characters", type=int, nargs="+", default=(0, 1, 2, 3, 4))
    search_evaluation.add_argument("--max-steps", type=int, default=2048)
    search_evaluation.add_argument("--max-combat-steps", type=int, default=512)
    search_evaluation.add_argument("--policy-temperature", type=float, default=.8)
    search_evaluation.add_argument("--simulations", type=int, default=128)
    search_evaluation.add_argument("--max-depth", type=int, default=64)
    search_evaluation.add_argument("--search-batch", type=int, default=256)
    search_evaluation.add_argument("--min-visits", type=int, default=16)
    search_evaluation.add_argument("--max-targets", type=int, default=64)
    search_evaluation.add_argument("--prior-temperature", type=float, default=3)
    search_evaluation.add_argument("--q-temperature", type=float, default=.002)
    search_evaluation.add_argument("--exploration", type=float, default=1.5)
    search_evaluation.add_argument("--exact-samples", type=int, default=16)
    search_evaluation.add_argument("--max-exact-states", type=int, default=100_000)
    search_evaluation.add_argument("--seed", type=int, default=4_620_000_000)
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
    search = commands.add_parser("search-diagnostics")
    search.add_argument("checkpoint")
    search.add_argument("--output", default="target/search-diagnostics-v68")
    search.add_argument("--roots", type=int, default=6)
    search.add_argument("--envs", type=int, default=64)
    search.add_argument("--max-steps", type=int, default=2048)
    search.add_argument("--sample-stride", type=int, default=64)
    search.add_argument("--simulations", type=int, default=128)
    search.add_argument("--max-depth", type=int, default=64)
    search.add_argument("--batch-size", type=int, default=256)
    search.add_argument("--prior-temperature", type=float, default=1)
    search.add_argument("--exploration", type=float, default=1.5)
    search.add_argument("--policy-temperature", type=float, default=.8)
    search.add_argument("--exact-samples", type=int, default=16)
    search.add_argument("--max-exact-states", type=int, default=100_000)
    search.add_argument("--max-pairs", type=int, default=200_000)
    search.add_argument("--progress-value", action="store_true")
    search.add_argument("--q-temperature", type=float, default=.002)
    search.add_argument("--seed", type=int, default=3_200_000_000)
    compare = commands.add_parser("compare-search")
    compare.add_argument("input", nargs="+")
    compare.add_argument("--output", default="target/search-diagnostics-comparison.html")
    critic = commands.add_parser("critic-diagnostics")
    critic.add_argument("checkpoint")
    critic.add_argument("--output", default="target/critic-diagnostics.json")
    critic.add_argument("--runs", type=int, default=64)
    critic.add_argument("--batch", type=int, default=32)
    critic.add_argument("--max-steps", type=int, default=2048)
    critic.add_argument("--max-combat-steps", type=int, default=512)
    critic.add_argument("--policy-temperature", type=float, default=.8)
    critic.add_argument("--precision", choices=PRECISIONS, default="fp32")
    critic.add_argument("--seed", type=int, default=3_600_000_000)
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
        report = evaluate(model, args, target)
        if args.output:
            immutable_json(Path(args.output), report)
        print(json.dumps(report, indent=2))
    elif args.command == "evaluate-search-policies":
        evaluate_search_policies(args)
    elif args.command == "dashboard":
        dashboard(Path(args.target))
    elif args.command == "search-diagnostics":
        diagnose_search(args)
    elif args.command == "compare-search":
        compare_search(args)
    elif args.command == "critic-diagnostics":
        critic_diagnostics(args)
    else:
        probe()
