#!/usr/bin/env python3
"""TCP server that sends plot waveform frames for the current xserial GUI."""

import argparse
import math
import socket
import struct
import threading
import time

PLOT_ESCAPE = 0x1E
PLOT_MARKER = ord("P")


def build_frame(
    sample_index: int,
    channels: int,
    plot_format: str,
    samples_per_channel: int,
    amplitude: float,
    frequency_hz: float,
    sample_rate_hz: float,
) -> bytes:
    values = []
    if plot_format == "xy":
        for offset in range(samples_per_channel):
            t = (sample_index + offset) / sample_rate_hz
            x = amplitude * math.cos(2 * math.pi * frequency_hz * t)
            y = amplitude * math.sin(2 * math.pi * frequency_hz * t)
            values.extend([x, y])
        return struct.pack(f"<{len(values)}f", *values)

    for offset in range(samples_per_channel):
        t = (sample_index + offset) / sample_rate_hz
        for ch in range(channels):
            phase = 2 * math.pi * ch / max(channels, 1)
            value = amplitude * math.sin(2 * math.pi * frequency_hz * t + phase)
            values.append(value)
    return struct.pack(f"<{len(values)}f", *values)


def cobs_encode(payload: bytes) -> bytes:
    if not payload:
        return b"\x01"

    out = bytearray([0])
    code_index = 0
    code = 1
    for byte in payload:
        if byte == 0:
            out[code_index] = code
            code_index = len(out)
            out.append(0)
            code = 1
        else:
            out.append(byte)
            code += 1
            if code == 0xFF:
                out[code_index] = code
                code_index = len(out)
                out.append(0)
                code = 1
    out[code_index] = code
    return bytes(out)


def build_plot_packet(
    payload: bytes,
    channels: int,
    samples_per_channel: int,
    plot_format: str,
) -> bytes:
    format_id = {"interleaved": 0, "block": 1, "xy": 2}[plot_format]
    header = bytearray()
    header.extend(b"XP")
    header.append(1)  # version
    header.append(format_id)
    header.append(8)  # f32
    header.append(0)  # little-endian
    header.append(channels)
    header.extend(struct.pack("<H", samples_per_channel))
    header.extend(struct.pack("<I", len(payload)))
    header.extend(payload)
    return bytes(header)


def build_mixed_plot_frame(
    payload: bytes,
    channels: int,
    samples_per_channel: int,
    plot_format: str,
) -> bytes:
    packet = build_plot_packet(
        payload=payload,
        channels=channels,
        samples_per_channel=samples_per_channel,
        plot_format=plot_format,
    )
    return bytes([PLOT_ESCAPE, PLOT_MARKER]) + cobs_encode(packet) + b"\x00"

def main():
    p = argparse.ArgumentParser(description="xserial plot test server")
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8080)
    p.add_argument("--channels", type=int, default=1)
    p.add_argument(
        "--format",
        choices=("interleaved", "xy"),
        default="interleaved",
        help="plot format (default: interleaved)",
    )
    p.add_argument(
        "--rate",
        type=float,
        default=1024,
        help="samples/sec per channel (default: 1024)",
    )
    p.add_argument("--freq", type=float, default=1.0, help="sine Hz")
    p.add_argument("--amp", type=float, default=100)
    p.add_argument(
        "--text-interval",
        type=float,
        default=1.0,
        help="seconds between status text messages (default: 1.0)",
    )
    p.add_argument(
        "--framelen",
        type=int,
        default=0,
        help="fixed frame bytes (0=one sample per channel per frame)",
    )
    p.add_argument(
        "--wire-format",
        choices=("mixed", "raw"),
        default="mixed",
        help="mixed sends text+plot on one connection; raw sends plot only",
    )
    args = p.parse_args()

    sample_size = 4  # f32

    if args.channels <= 0:
        raise SystemExit("--channels must be >= 1")
    if args.format == "xy" and args.channels != 2:
        raise SystemExit("--format xy requires --channels 2")
    if args.rate <= 0:
        raise SystemExit("--rate must be positive")
    if args.text_interval <= 0:
        raise SystemExit("--text-interval must be positive")
    if args.framelen < 0:
        raise SystemExit("--framelen must be >= 0")

    bytes_per_sample_group = args.channels * sample_size
    frame_bytes = args.framelen or bytes_per_sample_group
    if frame_bytes % bytes_per_sample_group != 0:
        raise SystemExit(
            f"--framelen must be a multiple of {bytes_per_sample_group} bytes "
            f"for {args.channels} channel f32 interleaved data"
        )
    samples_per_channel = frame_bytes // bytes_per_sample_group
    frame_interval = samples_per_channel / args.rate
    frame_rate = args.rate / samples_per_channel

    def sample_values(sample_index: int) -> list[float]:
        if args.format == "xy":
            t = sample_index / args.rate
            return [
                args.amp * math.cos(2 * math.pi * args.freq * t),
                args.amp * math.sin(2 * math.pi * args.freq * t),
            ]

        t = sample_index / args.rate
        values = []
        for ch in range(args.channels):
            phase = 2 * math.pi * ch / max(args.channels, 1)
            values.append(args.amp * math.sin(2 * math.pi * args.freq * t + phase))
        return values

    print(f"Listening {args.host}:{args.port} — Ctrl+C to stop")
    print("GUI:")
    print(f"  transport = TCP {args.host}:{args.port}")
    if args.wire_format == "mixed":
        print("  framer    = MixedTextPlot")
        print("  decoder   = MixedTextPlot\n")
    else:
        print(f"  framer    = Fixed({frame_bytes})")
        print(
            f"  decoder   = Plot(f32, little-endian, {args.channels} channel, {args.format})\n"
        )
    print(f"sending {samples_per_channel} samples/frame at {args.rate:.1f} samples/sec")
    print(f"effective frame rate: {frame_rate:.2f} fps\n")

    stop_event = threading.Event()

    def serve_stream() -> None:
        srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        srv.bind((args.host, args.port))
        srv.listen(1)
        srv.settimeout(0.5)
        try:
            while not stop_event.is_set():
                try:
                    conn, addr = srv.accept()
                except socket.timeout:
                    continue
                print(f"[conn +] {addr}")

                started_at = time.perf_counter()
                frame_count = 0
                text_count = 0
                sample_index = 0
                next_text_at = started_at
                try:
                    while not stop_event.is_set():
                        plot_payload = build_frame(
                            sample_index=sample_index,
                            channels=args.channels,
                            plot_format=args.format,
                            samples_per_channel=samples_per_channel,
                            amplitude=args.amp,
                            frequency_hz=args.freq,
                            sample_rate_hz=args.rate,
                        )
                        if args.wire_format == "mixed":
                            frame = build_mixed_plot_frame(
                                payload=plot_payload,
                                channels=args.channels,
                                samples_per_channel=samples_per_channel,
                                plot_format=args.format,
                            )
                        else:
                            frame = plot_payload
                        conn.sendall(frame)

                        now = time.perf_counter()
                        if args.wire_format == "mixed" and now >= next_text_at:
                            values = sample_values(sample_index)
                            joined = ", ".join(
                                f"ch{index + 1}={value:8.3f}"
                                for index, value in enumerate(values)
                            )
                            line = (
                                f"t={now - started_at:7.3f}s "
                                f"sample={sample_index:7d} {joined}\n"
                            )
                            conn.sendall(line.encode("utf-8"))
                            text_count += 1
                            next_text_at += args.text_interval

                        sample_index += samples_per_channel
                        frame_count += 1

                        elapsed = time.perf_counter() - started_at
                        wait = frame_count * frame_interval - elapsed
                        if wait > 0:
                            time.sleep(wait)
                except (BrokenPipeError, ConnectionResetError):
                    print(f"[conn -] {addr} ({frame_count} plot frames, {text_count} text lines)")
                finally:
                    conn.close()
        finally:
            srv.close()

    stream_thread = threading.Thread(target=serve_stream, daemon=True)
    stream_thread.start()

    try:
        while True:
            time.sleep(0.5)
    except KeyboardInterrupt:
        print("\nstopped.")
    finally:
        stop_event.set()
        stream_thread.join(timeout=1.0)

if __name__ == "__main__":
    main()
