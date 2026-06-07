#!/usr/bin/env python3
"""
SerialPlot 协议测试数据生成器

协议格式:
  [Sync: AA 55] [Size: 2B LE] [Payload: N×2B i16 LE] [Checksum: 1B LSB sum of payload]

用法:
  python3 gen_plot.py /dev/pts/X --channels 2 --wave sine
  python3 gen_plot.py /dev/pts/X --channels 2 --wave sine --freq 0.3  # 慢波，易观察
  python3 gen_plot.py /dev/pts/X --channels 3 --wave saw --freq 0.2
"""
import struct
import sys
import time
import math
import argparse


def checksum(data: bytes) -> int:
    return sum(data) & 0xFF


def make_frame(channel_data: list[list[int]]) -> bytes:
    num_channels = len(channel_data)
    num_samples = len(channel_data[0])
    payload = bytearray()
    for i in range(num_samples):
        for ch in range(num_channels):
            payload.extend(struct.pack('<h', channel_data[ch][i]))

    frame = bytearray([0xAA, 0x55])
    frame.append(len(payload) & 0xFF)
    frame.append((len(payload) >> 8) & 0xFF)
    frame.extend(payload)
    frame.append(checksum(payload))
    return bytes(frame)


def generate_wave(
    num_channels: int,
    num_samples: int,
    wave_type: str,
    freq: float,
    amplitude: int,
) -> list[list[int]]:
    channels = []
    for ch in range(num_channels):
        phase_offset = ch * (2 * math.pi / num_channels)
        samples = []
        for i in range(num_samples):
            t = i / num_samples
            if wave_type == 'sine':
                angle = 2 * math.pi * t * freq + phase_offset
                value = math.sin(angle)
            elif wave_type == 'saw':
                value = 2 * ((t * freq) % 1.0) - 1.0
            elif wave_type == 'square':
                value = 1.0 if ((t * freq) % 2.0) < 1.0 else -1.0
            else:
                value = 0.0
            samples.append(int(value * amplitude))
        channels.append(samples)
    return channels


def main():
    parser = argparse.ArgumentParser(description='SerialPlot test data generator')
    parser.add_argument('port', help='Serial port or file (e.g. /dev/pts/3)')
    parser.add_argument('--channels', type=int, default=2, help='Number of channels')
    parser.add_argument('--samples', type=int, default=200, help='Samples per frame')
    parser.add_argument('--wave', choices=['sine', 'saw', 'square'], default='sine')
    parser.add_argument('--freq', type=float, default=0.5,
                        help='Wave frequency (cycles per frame; 0.5 = half a sine wave, easy to observe)')
    parser.add_argument('--amplitude', type=int, default=8000,
                        help='Amplitude (i16 range, max 32767)')
    parser.add_argument('--interval', type=float, default=0.05,
                        help='Interval between frames (seconds)')
    parser.add_argument('--frames', type=int, default=0,
                        help='Number of frames (0=infinite)')
    args = parser.parse_args()

    count = 0
    try:
        with open(args.port, 'wb') as f:
            print(f"Generating {args.channels}-channel {args.wave} wave → {args.port}")
            print(f"  {args.samples} samples/frame, freq={args.freq} cycles/frame")
            print(f"  amplitude={args.amplitude}, interval={args.interval}s")
            print(f"  Protocol: sync=AA55, i16 LE, 1B size+checksum")
            print()

            while args.frames == 0 or count < args.frames:
                count += 1
                data = generate_wave(args.channels, args.samples, args.wave, args.freq, args.amplitude)
                frame = make_frame(data)
                f.write(frame)
                f.flush()

                vals = [data[ch][0] for ch in range(args.channels)]
                vals_str = ", ".join(f"ch{ch}={v:+6d}" for ch, v in enumerate(vals))
                print(f"  Frame #{count}: {vals_str} ... ({len(frame)} bytes)", end='\r')

                time.sleep(args.interval)
    except KeyboardInterrupt:
        print(f"\nDone. {count} frames sent.")
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == '__main__':
    main()
