/*
 * test_text_serial.c — 持续向串口发送纯文本正弦波数据流
 *
 * 编译 (Linux):
 *   gcc -std=c11 -Wall -Wextra -O2 -o test_text_serial tools/test_text_serial.c -lm
 *
 * 用法:
 *   ./test_text_serial /dev/ttyUSB0
 *   ./test_text_serial /dev/ttyUSB0 --baud 115200 --channels 2 --rate 100 --freq 2.0 --amp 150
 *   ./test_text_serial --help
 *
 * 输出格式 (每行):
 *   sample=123 ch1=100.000 ch2=0.000
 *
 * 串口参数: 8N1, 无流控, 默认 115200 baud
 */

#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <fcntl.h>
#include <getopt.h>
#include <math.h>
#include <signal.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

#define DEFAULT_BAUDRATE      115200
#define DEFAULT_RATE          100.0      /* lines/sec */
#define DEFAULT_FREQ          1.0
#define DEFAULT_AMP           100.0
#define MAX_CHANNELS          8
#define LINE_BUF_SIZE         512

static volatile sig_atomic_t g_running = 1;

static void sigint_handler(int sig) {
    (void)sig;
    g_running = 0;
}

typedef struct {
    const char *port;
    int         baudrate;
    uint8_t     channels;
    double      rate;
    double      freq;
    double      amp;
} args_t;

static int serial_open(const char *path, int baudrate) {
    int fd = open(path, O_RDWR | O_NOCTTY | O_NONBLOCK);
    if (fd < 0) { perror("open"); return -1; }

    int flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0) { perror("fcntl"); close(fd); return -1; }
    flags &= ~O_NONBLOCK;
    if (fcntl(fd, F_SETFL, flags) < 0) { perror("fcntl"); close(fd); return -1; }

    struct termios tty;
    memset(&tty, 0, sizeof(tty));
    if (tcgetattr(fd, &tty) < 0) { perror("tcgetattr"); close(fd); return -1; }

    tty.c_iflag = IGNBRK;
    tty.c_oflag = 0;
    tty.c_cflag = CS8 | CREAD | CLOCAL;
    tty.c_lflag = 0;
    tty.c_cc[VMIN]  = 1;
    tty.c_cc[VTIME] = 0;

    speed_t speed = B115200;
    switch (baudrate) {
        case 9600:    speed = B9600;    break;
        case 19200:   speed = B19200;   break;
        case 38400:   speed = B38400;   break;
        case 57600:   speed = B57600;   break;
        case 115200:  speed = B115200;  break;
        case 230400:  speed = B230400;  break;
        case 460800:  speed = B460800;  break;
        case 921600:  speed = B921600;  break;
        default:
            fprintf(stderr, "不支持的波特率: %d\n", baudrate);
            close(fd);
            return -1;
    }
    cfsetispeed(&tty, speed);
    cfsetospeed(&tty, speed);

#ifdef CRTSCTS
    tty.c_cflag &= ~CRTSCTS;
#endif
    tty.c_iflag &= ~(IXON | IXOFF | IXANY);

    if (tcsetattr(fd, TCSANOW, &tty) < 0) {
        perror("tcsetattr");
        close(fd);
        return -1;
    }
    tcflush(fd, TCIOFLUSH);
    return fd;
}

static double now_sec(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec * 1e-9;
}

static void sleep_sec(double seconds) {
    if (seconds <= 0.0) return;
    struct timespec ts;
    ts.tv_sec  = (time_t)seconds;
    ts.tv_nsec = (long)((seconds - (time_t)seconds) * 1e9);
    clock_nanosleep(CLOCK_MONOTONIC, 0, &ts, NULL);
}

static int format_line(char *buf, size_t cap, uint32_t sample, const float *values, uint8_t channels) {
    int pos = snprintf(buf, cap, "sample=%-7u", sample);
    for (uint8_t ch = 0; ch < channels; ++ch) {
        pos += snprintf(buf + pos, cap - (size_t)pos, " ch%d=%8.3f", ch + 1, (double)values[ch]);
    }
    pos += snprintf(buf + pos, cap - (size_t)pos, "\n");
    return pos >= (int)cap ? (int)cap - 1 : pos;
}

static void print_usage(const char *prog) {
    printf("用法: %s <串口> [选项]\n", prog);
    printf("\n选项:\n");
    printf("  --baud <n>      波特率 (默认 %d)\n", DEFAULT_BAUDRATE);
    printf("  --channels <n>  通道数 (默认 1)\n");
    printf("  --rate <f>      行/秒 输出速率 (默认 %.0f)\n", DEFAULT_RATE);
    printf("  --freq <f>      正弦频率 Hz (默认 %.1f)\n", DEFAULT_FREQ);
    printf("  --amp <f>       振幅 (默认 %.0f)\n", DEFAULT_AMP);
    printf("  -h, --help      显示帮助\n");
    printf("\n示例:\n");
    printf("  %s /dev/ttyUSB0\n", prog);
    printf("  %s /dev/ttyUSB0 --channels 2 --rate 50 --freq 3\n", prog);
    printf("  %s /dev/ttyUSB0 --baud 921600 --rate 1000\n", prog);
}

static int parse_args(int argc, char **argv, args_t *args) {
    memset(args, 0, sizeof(*args));
    args->baudrate   = DEFAULT_BAUDRATE;
    args->channels   = 1;
    args->rate       = DEFAULT_RATE;
    args->freq       = DEFAULT_FREQ;
    args->amp        = DEFAULT_AMP;

    enum { OPT_BAUD = 256, OPT_CHANNELS, OPT_RATE, OPT_FREQ, OPT_AMP };

    static struct option long_opts[] = {
        {"baud",     required_argument, 0, OPT_BAUD},
        {"channels", required_argument, 0, OPT_CHANNELS},
        {"rate",     required_argument, 0, OPT_RATE},
        {"freq",     required_argument, 0, OPT_FREQ},
        {"amp",      required_argument, 0, OPT_AMP},
        {"help",     no_argument,       0, 'h'},
        {0, 0, 0, 0}
    };

    int opt;
    while ((opt = getopt_long(argc, argv, "h", long_opts, NULL)) != -1) {
        switch (opt) {
        case OPT_BAUD:     args->baudrate = atoi(optarg); break;
        case OPT_CHANNELS: {
            int v = atoi(optarg);
            if (v < 1 || v > MAX_CHANNELS) {
                fprintf(stderr, "通道数必须在 1-%d 之间\n", MAX_CHANNELS);
                return -1;
            }
            args->channels = (uint8_t)v;
            break;
        }
        case OPT_RATE:     args->rate = atof(optarg); break;
        case OPT_FREQ:     args->freq = atof(optarg); break;
        case OPT_AMP:      args->amp = atof(optarg); break;
        case 'h':          print_usage(argv[0]); exit(0);
        default:           print_usage(argv[0]); return -1;
        }
    }

    if (optind >= argc) {
        fprintf(stderr, "错误: 缺少串口路径\n");
        print_usage(argv[0]);
        return -1;
    }
    args->port = argv[optind];

    if (args->rate <= 0.0)  { fprintf(stderr, "错误: --rate 必须为正数\n");  return -1; }

    return 0;
}

int main(int argc, char **argv) {
    args_t args;
    if (parse_args(argc, argv, &args) < 0) return 1;

    double interval = 1.0 / args.rate;

    printf("═══ pipeview Plain Text Generator ═══\n");
    printf("串口:   %s @ %d baud\n", args.port, args.baudrate);
    printf("通道:   %u  振幅: %.1f  频率: %.1f Hz\n", args.channels, args.amp, args.freq);
    printf("速率:   %.0f lines/sec  (间隔 %.3f ms)\n", args.rate, interval * 1000.0);
    printf("═════════════════════════════════════\n");

    float values[MAX_CHANNELS];
    char  line[LINE_BUF_SIZE];

    int fd = serial_open(args.port, args.baudrate);
    if (fd < 0) { fprintf(stderr, "无法打开串口 %s\n", args.port); return 1; }
    printf("已打开 %s\n\n开始发送 (Ctrl+C 停止) ...\n", args.port);

    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_handler = sigint_handler;
    sigaction(SIGINT, &sa, NULL);
    sigaction(SIGTERM, &sa, NULL);

    uint32_t sample_index = 0;
    uint64_t line_count   = 0;
    double   started_at   = now_sec();

    while (g_running) {
        double t = (double)sample_index / args.rate;
        for (uint8_t ch = 0; ch < args.channels; ++ch) {
            double phase = 2.0 * M_PI * (double)ch / (double)args.channels;
            values[ch] = (float)(args.amp * sin(2.0 * M_PI * args.freq * t + phase));
        }

        int len = format_line(line, sizeof(line), sample_index, values, args.channels);
        size_t written = 0;
        while (written < (size_t)len) {
            ssize_t n = write(fd, line + written, (size_t)len - written);
            if (n < 0) { perror("write"); goto cleanup; }
            written += (size_t)n;
        }
        line_count++;

        double elapsed = now_sec() - started_at;
        double target = (double)line_count * interval;
        double wait = target - elapsed;
        if (wait > 0.0) sleep_sec(wait);

        sample_index++;
    }

cleanup: {
    double total_time = now_sec() - started_at;
    printf("\n═══ 已停止 ═══\n");
    printf("运行时间: %.1f 秒\n", total_time);
    printf("行数:     %lu  (%.1f lines/sec)\n",
           (unsigned long)line_count,
           total_time > 0.0 ? (double)line_count / total_time : 0.0);

    tcflush(fd, TCIOFLUSH);
    close(fd);
    return 0;
}
}
