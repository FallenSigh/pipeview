#!/usr/bin/env python3
"""Generate fake drone telemetry data for testing pipeview Lua decoder."""

import argparse
import math
import random
import socket
import time


def generate_line(t: float) -> str:
    """Generate one telemetry line with simulated sensor values."""
    # Quaternion (w, x, y, z) — gently rotating
    angle = t * 0.5
    qw = math.cos(angle)
    qx = math.sin(angle) * 0.1
    qy = math.sin(angle) * 0.05
    qz = math.sin(angle) * 0.02

    # Yaw/Pitch/Roll — sine wave simulation
    yaw = math.degrees(math.sin(t * 0.3)) * 30
    pitch = math.degrees(math.sin(t * 0.5)) * 15
    roll = math.degrees(math.sin(t * 0.7)) * 10

    # Gyro (deg/s)
    gz = math.sin(t * 0.3) * 50 + random.uniform(-2, 2)
    gy = math.sin(t * 0.5) * 30 + random.uniform(-2, 2)
    gx = math.sin(t * 0.7) * 20 + random.uniform(-2, 2)

    # RC channels (1000-2000 us)
    rc_r = int(1500 + math.sin(t * 0.3) * 200)
    rc_p = int(1500 + math.sin(t * 0.5) * 150)
    rc_t = int(1000 + (math.sin(t * 0.1) + 1) * 500)  # throttle 1000-2000
    rc_y = int(1500 + math.sin(t * 0.3) * 100)

    # Motors (1000-2000)
    base = 1200 + int((math.sin(t * 0.1) + 1) * 400)
    m1 = base + random.randint(-20, 20)
    m2 = base + random.randint(-20, 20)
    m3 = base + random.randint(-20, 20)
    m4 = base + random.randint(-20, 20)

    return (
        f"AHRS q:{qw:.4f},{qx:.4f},{qy:.4f},{qz:.4f}|"
        f"YPR:{yaw:.2f},{pitch:.2f},{roll:.2f}|"
        f"Gyro:{gz:.2f},{gy:.2f},{gx:.2f}|"
        f"RC:{rc_r},{rc_p},{rc_t},{rc_y}|"
        f"M:{m1},{m2},{m3},{m4}|"
        f"L:0 F:1 C:0\n"
    )


def main():
    parser = argparse.ArgumentParser(description="Drone telemetry test data generator")
    parser.add_argument("--host", default="127.0.0.1", help="TCP host")
    parser.add_argument("--port", type=int, default=8092, help="TCP port")
    parser.add_argument("--rate", type=float, default=10, help="Lines per second")
    parser.add_argument("--duration", type=float, default=0, help="Seconds to run (0 = forever)")
    args = parser.parse_args()

    interval = 1.0 / args.rate
    print(f"Sending drone telemetry to {args.host}:{args.port} at {args.rate} Hz")

    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as server:
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind((args.host, args.port))
        server.listen(1)
        print(f"Listening on {args.host}:{args.port}, waiting for connections...")

        while True:
            sock, addr = server.accept()
            print(f"Client connected from {addr}")

            t0 = time.time()
            seq = 0

            try:
                while True:
                    t = time.time() - t0
                    line = generate_line(t)
                    try:
                        sock.sendall(line.encode())
                    except (BrokenPipeError, ConnectionResetError):
                        break
                    seq += 1

                    if args.duration > 0 and t >= args.duration:
                        break

                    elapsed = time.time() - t0
                    target = (seq + 1) * interval
                    sleep_time = target - elapsed
                    if sleep_time > 0:
                        time.sleep(sleep_time)
            except (BrokenPipeError, ConnectionResetError):
                pass

            print(f"Client disconnected. Sent {seq} lines. Waiting for new connection...")


if __name__ == "__main__":
    main()
