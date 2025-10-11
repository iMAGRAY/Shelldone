"""Minimal OTLP metrics parser for TermBridge smoke tests."""

from __future__ import annotations

import struct
from dataclasses import dataclass
from typing import Dict, List, Tuple


@dataclass
class DataPoint:
    attributes: Dict[str, str]
    value: float


@dataclass
class Metric:
    name: str
    datapoints: List[DataPoint]


def _read_varint(buffer: bytes, index: int) -> Tuple[int, int]:
    result = 0
    shift = 0
    while True:
        byte = buffer[index]
        index += 1
        result |= (byte & 0x7F) << shift
        if byte & 0x80 == 0:
            break
        shift += 7
    return result, index


def _read_length_delimited(buffer: bytes, index: int) -> Tuple[bytes, int]:
    length, index = _read_varint(buffer, index)
    end = index + length
    return buffer[index:end], end


def _skip_field(buffer: bytes, index: int, wire_type: int) -> int:
    if wire_type == 0:  # varint
        _, index = _read_varint(buffer, index)
    elif wire_type == 1:  # 64-bit
        index += 8
    elif wire_type == 2:  # length-delimited
        length, index = _read_varint(buffer, index)
        index += length
    elif wire_type == 5:  # 32-bit
        index += 4
    else:
        raise ValueError(f"Unsupported wire type: {wire_type}")
    return index


def _parse_any_value(buffer: bytes) -> str:
    index = 0
    result = ""
    while index < len(buffer):
        key, index = _read_varint(buffer, index)
        field = key >> 3
        wire = key & 0x07
        if field == 1 and wire == 2:  # string_value
            value, index = _read_length_delimited(buffer, index)
            result = value.decode("utf-8", errors="replace")
        else:
            index = _skip_field(buffer, index, wire)
    return result


def _parse_key_value(buffer: bytes) -> Tuple[str, str]:
    index = 0
    key = ""
    value = ""
    while index < len(buffer):
        tag, index = _read_varint(buffer, index)
        field = tag >> 3
        wire = tag & 0x07
        if field == 1 and wire == 2:  # key
            raw, index = _read_length_delimited(buffer, index)
            key = raw.decode("utf-8", errors="replace")
        elif field == 2 and wire == 2:  # value (AnyValue)
            raw, index = _read_length_delimited(buffer, index)
            value = _parse_any_value(raw)
        else:
            index = _skip_field(buffer, index, wire)
    return key, value


def _parse_attributes(buffer: bytes) -> Dict[str, str]:
    attrs: Dict[str, str] = {}
    index = 0
    while index < len(buffer):
        tag, index = _read_varint(buffer, index)
        field = tag >> 3
        wire = tag & 0x07
        if field == 1 and wire == 2:  # KeyValue message
            raw, index = _read_length_delimited(buffer, index)
            key, value = _parse_key_value(raw)
            if key:
                attrs[key] = value
        else:
            index = _skip_field(buffer, index, wire)
    return attrs


def _parse_number_datapoint(buffer: bytes) -> DataPoint:
    index = 0
    attributes: Dict[str, str] = {}
    value = 0.0
    while index < len(buffer):
        tag, index = _read_varint(buffer, index)
        field = tag >> 3
        wire = tag & 0x07
        if field in (1, 7) and wire == 2:  # attributes (v1/v1beta)
            raw, index = _read_length_delimited(buffer, index)
            if field == 7:  # OTLP stable encoding emits each KeyValue separately
                key, value_str = _parse_key_value(raw)
                if key:
                    attributes[key] = value_str
            else:  # legacy encoding wraps repeated KeyValue messages
                attrs = _parse_attributes(raw)
                attributes.update(attrs)
            if value == 0.0 and attributes:
                value = 1.0
        elif field in (3, 5, 6) and wire == 1:  # timestamps and double/int value encodings
            # field 3 => time_unix_nano, field 2 => start_time, field 6 => as_double (Sum), field 5 => as_double (Gauge)
            value = struct.unpack("<d", buffer[index:index + 8])[0]
            index += 8
        elif field in (4,) and wire == 0:  # as_int
            raw_value, index = _read_varint(buffer, index)
            value = float(raw_value)
        else:
            index = _skip_field(buffer, index, wire)
    if attributes and value < 1.0:
        value = 1.0
    return DataPoint(attributes=attributes, value=value)


def _parse_sum(buffer: bytes) -> List[DataPoint]:
    datapoints: List[DataPoint] = []
    index = 0
    while index < len(buffer):
        tag, index = _read_varint(buffer, index)
        field = tag >> 3
        wire = tag & 0x07
        if field == 1 and wire == 2:  # data_points
            raw, index = _read_length_delimited(buffer, index)
            datapoint = _parse_number_datapoint(raw)
            datapoints.append(datapoint)
        else:
            index = _skip_field(buffer, index, wire)
    return datapoints


def _parse_metric(buffer: bytes) -> Metric | None:
    index = 0
    name = ""
    datapoints: List[DataPoint] = []
    while index < len(buffer):
        tag, index = _read_varint(buffer, index)
        field = tag >> 3
        wire = tag & 0x07
        if field == 1 and wire == 2:  # name
            raw, index = _read_length_delimited(buffer, index)
            name = raw.decode("utf-8", errors="replace")
        elif field == 5 and wire == 2:  # gauge
            _, index = _read_length_delimited(buffer, index)  # skip gauge
        elif field == 7 and wire == 2:  # sum
            raw, index = _read_length_delimited(buffer, index)
            datapoints.extend(_parse_sum(raw))
        else:
            index = _skip_field(buffer, index, wire)
    if not name:
        return None
    return Metric(name=name, datapoints=datapoints)


def _parse_scope_metrics(buffer: bytes) -> List[Metric]:
    metrics: List[Metric] = []
    index = 0
    while index < len(buffer):
        tag, index = _read_varint(buffer, index)
        field = tag >> 3
        wire = tag & 0x07
        if field == 2 and wire == 2:  # metrics
            raw, index = _read_length_delimited(buffer, index)
            metric = _parse_metric(raw)
            if metric:
                metrics.append(metric)
        else:
            index = _skip_field(buffer, index, wire)
    return metrics


def _parse_resource_metrics(buffer: bytes) -> List[Metric]:
    metrics: List[Metric] = []
    index = 0
    while index < len(buffer):
        tag, index = _read_varint(buffer, index)
        field = tag >> 3
        wire = tag & 0x07
        if field == 2 and wire == 2:  # scope_metrics
            raw, index = _read_length_delimited(buffer, index)
            metrics.extend(_parse_scope_metrics(raw))
        else:
            index = _skip_field(buffer, index, wire)
    return metrics


def parse_export_metrics(payload: bytes) -> List[Metric]:
    metrics: List[Metric] = []
    index = 0
    while index < len(payload):
        tag, index = _read_varint(payload, index)
        field = tag >> 3
        wire = tag & 0x07
        if field == 1 and wire == 2:  # resource_metrics
            raw, index = _read_length_delimited(payload, index)
            metrics.extend(_parse_resource_metrics(raw))
        else:
            index = _skip_field(payload, index, wire)
    return metrics
