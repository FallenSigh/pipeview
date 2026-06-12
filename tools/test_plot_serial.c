/*
 * test_plot_serial.c — 持续向串口发送 MixedTextPlot 正弦波 + 文本流
 *
 * 编译 (Linux):
 *   gcc -std=c11 -Wall -Wextra -O2 -o test_plot_serial tools/test_plot_serial.c -lm
 *
 * 用法:
 *   ./test_plot_serial /dev/ttyUSB0
 *   ./test_plot_serial /dev/ttyUSB0 --baud 115200 --channels 2 --format xy --rate 1024 --freq 2.0 --amp 150
 *   ./test_plot_serial --help
 *
 * 依赖: tools/xs_mixed_plot.h (同目录, 定义 COBS 和 MixedTextPlot 帧格式)
 *
 * 协议说明:
 *   Plot 帧:  0x1E 'P' + COBS(packet) + 0x00
 *   文本帧:   UTF-8 文本行 + '\n' (直接写入, 由 MixedTextPlot framer 按行分割)
 *
 * 串口参数: 8N1, 无流控, 默认 115200 baud
 */

#define _POSIX_C_SOURCE 200809L
#include "xs_mixed_plot.h"

#include <errno.h>
#include <fcntl.h>
#include <getopt.h>
#include <math.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

/* ── 配置默认值 ───────────────────────────────────────────────────── */

#define DEFAULT_BAUDRATE    115200
#define DEFAULT_RATE        1024.0     /* samples/sec per channel */
#define DEFAULT_FREQ        1.0        /* sine Hz */
#define DEFAULT_AMP         100.0
#define DEFAULT_TEXT_INTERVAL 1.0     /* seconds between text lines */

/* ── 全局状态 (用于信号处理) ──────────────────────────────────────── */

static volatile sig_atomic_t g_running = 1;

static void sigint_handler(int sig) {
    (void)sig;
    g_running = 0;
}

/* ── 缓冲大小 ─────────────────────────────────────────────────────── */

#define MAX_CHANNELS         8
#define MAX_SPC              4096       /* samples_per_channel */
#define MAX_VALUES           (MAX_CHANNELS * MAX_SPC)  /* float values per frame */
#define MAX_PAYLOAD          (MAX_VALUES * 4)
#define MAX_PACKET           (13 + MAX_PAYLOAD)
#define MAX_FRAME            (2 + (MAX_PACKET + MAX_PACKET / 254 + 1) + 1)
#define TEXT_BUF_SIZE        512

/* ── 运行时参数 ───────────────────────────────────────────────────── */

typedef struct {
    const char *port;
    int         baudrate;
    uint8_t     channels;
    uint16_t    samples_per_channel;
    uint8_t     plot_format;    /* XS_PLOT_FORMAT_INTERLEAVED / XY */
    double      rate;           /* samples/sec */
    double      freq;           /* sine Hz */
    double      amp;
    double      text_interval;  /* seconds */
} args_t;

/* ── 串口操作 ─────────────────────────────────────────────────────── */

static int serial_open(const char *path, int baudrate) {
    int fd = open(path, O_RDWR | O_NOCTTY | O_NONBLOCK);
    if (fd < 0) {
        perror("open");
        return -1;
    }

    /* 恢复阻塞模式, 但配合超时使用 select */
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0) { perror("fcntl"); close(fd); return -1; }
    flags &= ~O_NONBLOCK;
    if (fcntl(fd, F_SETFL, flags) < 0) { perror("fcntl"); close(fd); return -1; }

    struct termios tty;
    memset(&tty, 0, sizeof(tty));
    if (tcgetattr(fd, &tty) < 0) { perror("tcgetattr"); close(fd); return -1; }

    /* 输入: 忽略 BREAK, CR→NL 不转换, 奇偶校验不检查 */
    tty.c_iflag = IGNBRK;

    /* 输出: 原始模式, 不做任何转换 */
    tty.c_oflag = 0;

    /* 控制: 8 位, 无奇偶校验, 启用接收器 */
    tty.c_cflag = CS8 | CREAD | CLOCAL;

    /* 本地: 关闭标准行处理/回显/信号字符 */
    tty.c_lflag = 0;

    /* 特殊字符: 无超时 (VMIN=1, VTIME=0 → 阻塞读直到至少1字节) */
    tty.c_cc[VMIN]  = 1;
    tty.c_cc[VTIME] = 0;

    /* 设置波特率 */
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
            fprintf(stderr, "不支持的波特率: %d (支持: 9600-921600)\n", baudrate);
            close(fd);
            return -1;
    }
    cfsetispeed(&tty, speed);
    cfsetospeed(&tty, speed);

#ifdef CRTSCTS
    tty.c_cflag &= ~CRTSCTS;
#endif
    /* 关闭软件流控 */
    tty.c_iflag &= ~(IXON | IXOFF | IXANY);

    if (tcsetattr(fd, TCSANOW, &tty) < 0) {
        perror("tcsetattr");
        close(fd);
        return -1;
    }

    /* 排空缓冲区 */
    tcflush(fd, TCIOFLUSH);

    return fd;
}

/* ── 时间工具 ─────────────────────────────────────────────────────── */

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

/* ── 信号生成 ─────────────────────────────────────────────────────── */

static void generate_samples(
    float *samples,
    uint32_t sample_index,
    uint8_t channels,
    uint16_t samples_per_channel,
    uint8_t plot_format,
    double rate,
    double freq,
    double amp)
{
    if (plot_format == XS_PLOT_FORMAT_XY) {
        /* XY 模式: 每对 (cos, sin) 画一个圆 */
        for (uint16_t i = 0; i < samples_per_channel; ++i) {
            double t = (double)(sample_index + i) / rate;
            samples[2 * i]     = (float)(amp * cos(2.0 * M_PI * freq * t));
            samples[2 * i + 1] = (float)(amp * sin(2.0 * M_PI * freq * t));
        }
    } else {
        /* interleaved 模式: 每个通道相位偏移 */
        for (uint16_t i = 0; i < samples_per_channel; ++i) {
            double t = (double)(sample_index + i) / rate;
            for (uint8_t ch = 0; ch < channels; ++ch) {
                double phase = 2.0 * M_PI * (double)ch / (double)channels;
                samples[i * channels + ch] = (float)(amp * sin(2.0 * M_PI * freq * t + phase));
            }
        }
    }
}

/* ── 用法 ─────────────────────────────────────────────────────────── */

static void print_usage(const char *prog) {
    printf("用法: %s <串口> [选项]\n", prog);
    printf("\n选项:\n");
    printf("  --baud <n>          波特率 (默认 %d)\n", DEFAULT_BAUDRATE);
    printf("  --channels <n>      通道数 (默认 1, --format xy 时强制 2)\n");
    printf("  --format <fmt>      格式: interleaved | xy (默认 interleaved)\n");
    printf("  --rate <f>          采样率, samples/sec (默认 %.0f)\n", DEFAULT_RATE);
    printf("  --freq <f>          正弦频率 Hz (默认 %.1f)\n", DEFAULT_FREQ);
    printf("  --amp <f>           振幅 (默认 %.0f)\n", DEFAULT_AMP);
    printf("  --text-interval <f> 文本输出间隔秒 (默认 %.1f)\n", DEFAULT_TEXT_INTERVAL);
    printf("  --spc <n>           每帧每通道采样数 (默认 1)\n");
    printf("  -h, --help          显示帮助\n");
    printf("\n支持的波特率: 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600\n");
    printf("\n示例:\n");
    printf("  %s /dev/ttyUSB0\n", prog);
    printf("  %s /dev/ttyUSB0 --baud 921600 --channels 2 --rate 2048 --freq 5\n", prog);
    printf("  %s /dev/ttyUSB0 --format xy --amp 200 --text-interval 2.0\n", prog);
    printf("  %s /dev/ttyUSB0 --channels 4 --spc 128 --rate 1024\n", prog);
}

/* ── 参数解析 ─────────────────────────────────────────────────────── */

static int parse_args(int argc, char **argv, args_t *args) {
    memset(args, 0, sizeof(*args));
    args->baudrate       = DEFAULT_BAUDRATE;
    args->channels       = 1;
    args->samples_per_channel = 1;
    args->plot_format    = XS_PLOT_FORMAT_INTERLEAVED;
    args->rate           = DEFAULT_RATE;
    args->freq           = DEFAULT_FREQ;
    args->amp            = DEFAULT_AMP;
    args->text_interval  = DEFAULT_TEXT_INTERVAL;

    enum { OPT_BAUD = 256, OPT_CHANNELS, OPT_FORMAT, OPT_RATE, OPT_FREQ,
           OPT_AMP, OPT_TEXT_INTERVAL, OPT_SPC };

    static struct option long_opts[] = {
        {"baud",          required_argument, 0, OPT_BAUD},
        {"channels",      required_argument, 0, OPT_CHANNELS},
        {"format",        required_argument, 0, OPT_FORMAT},
        {"rate",          required_argument, 0, OPT_RATE},
        {"freq",          required_argument, 0, OPT_FREQ},
        {"amp",           required_argument, 0, OPT_AMP},
        {"text-interval", required_argument, 0, OPT_TEXT_INTERVAL},
        {"spc",           required_argument, 0, OPT_SPC},
        {"help",          no_argument,       0, 'h'},
        {0, 0, 0, 0}
    };

    int opt;
    while ((opt = getopt_long(argc, argv, "h", long_opts, NULL)) != -1) {
        switch (opt) {
        case OPT_BAUD:
            args->baudrate = atoi(optarg);
            break;
        case OPT_CHANNELS: {
            int v = atoi(optarg);
            if (v < 1 || v > MAX_CHANNELS) {
                fprintf(stderr, "通道数必须在 1-%d 之间\n", MAX_CHANNELS);
                return -1;
            }
            args->channels = (uint8_t)v;
            break;
        }
        case OPT_FORMAT:
            if (strcmp(optarg, "xy") == 0) {
                args->plot_format = XS_PLOT_FORMAT_XY;
            } else if (strcmp(optarg, "interleaved") == 0) {
                args->plot_format = XS_PLOT_FORMAT_INTERLEAVED;
            } else {
                fprintf(stderr, "未知格式: %s (支持: interleaved, xy)\n", optarg);
                return -1;
            }
            break;
        case OPT_RATE:
            args->rate = atof(optarg);
            break;
        case OPT_FREQ:
            args->freq = atof(optarg);
            break;
        case OPT_AMP:
            args->amp = atof(optarg);
            break;
        case OPT_TEXT_INTERVAL:
            args->text_interval = atof(optarg);
            break;
        case OPT_SPC: {
            int v = atoi(optarg);
            if (v < 1 || v > MAX_SPC) {
                fprintf(stderr, "spc 必须在 1-%d 之间\n", MAX_SPC);
                return -1;
            }
            args->samples_per_channel = (uint16_t)v;
            break;
        }
        case 'h':
            print_usage(argv[0]);
            exit(0);
        default:
            print_usage(argv[0]);
            return -1;
        }
    }

    if (optind >= argc) {
        fprintf(stderr, "错误: 缺少串口路径\n");
        print_usage(argv[0]);
        return -1;
    }
    args->port = argv[optind];

    /* 校验 */
    if (args->plot_format == XS_PLOT_FORMAT_XY) {
        if (args->channels != 2) {
            fprintf(stderr, "错误: --format xy 要求 --channels 2\n");
            return -1;
        }
    }
    if (args->channels == 0) {
        fprintf(stderr, "错误: --channels 必须 >= 1\n");
        return -1;
    }
    if (args->rate <= 0.0) {
        fprintf(stderr, "错误: --rate 必须为正数\n");
        return -1;
    }
    if (args->text_interval <= 0.0) {
        fprintf(stderr, "错误: --text-interval 必须为正数\n");
        return -1;
    }

    return 0;
}

/* ── 发送 plot 帧 ─────────────────────────────────────────────────── */

static int send_plot_frame(
    int fd,
    const float *samples,
    uint8_t channels,
    uint16_t samples_per_channel,
    uint8_t plot_format,
    uint8_t *packet_buf,
    size_t packet_cap,
    uint8_t *frame_buf,
    size_t frame_cap)
{
    size_t frame_len = 0;
    xs_status_t rc = xs_mixed_build_plot_frame_f32(
        samples, channels, samples_per_channel, plot_format,
        packet_buf, packet_cap,
        frame_buf, frame_cap,
        &frame_len);
    if (rc != XS_OK) {
        fprintf(stderr, "构建 plot 帧失败: %d\n", rc);
        return -1;
    }

    size_t written = 0;
    while (written < frame_len) {
        ssize_t n = write(fd, frame_buf + written, frame_len - written);
        if (n < 0) {
            perror("write");
            return -1;
        }
        written += (size_t)n;
    }
    return 0;
}

/* ── 发送文本行 ───────────────────────────────────────────────────── */

static int send_text_line(
    int fd,
    uint32_t sample_index,
    const float *current_samples,
    uint8_t channels,
    uint16_t samples_per_channel,
    const char *format_name)
{
    char buf[TEXT_BUF_SIZE];
    int pos = 0;

    pos += snprintf(buf + pos, sizeof(buf) - pos,
                    "sample=%-7u", sample_index);

    if (channels > 1) {
        for (uint8_t ch = 0; ch < channels; ++ch) {
            pos += snprintf(buf + pos, sizeof(buf) - pos,
                            " ch%d=%8.3f", ch + 1, (double)current_samples[ch]);
        }
    } else {
        pos += snprintf(buf + pos, sizeof(buf) - pos,
                        " value=%8.3f", (double)current_samples[0]);
    }

    pos += snprintf(buf + pos, sizeof(buf) - pos,
                    " [fmt=%s, ch=%u, spc=%u]\n",
                    format_name, channels, samples_per_channel);

    /* 确保不超过缓冲 */
    if (pos >= (int)sizeof(buf)) pos = (int)sizeof(buf) - 1;

    size_t len = (size_t)pos;
    size_t written = 0;
    while (written < len) {
        ssize_t n = write(fd, buf + written, len - written);
        if (n < 0) {
            perror("write");
            return -1;
        }
        written += (size_t)n;
    }
    return 0;
}

/* ── 主函数 ───────────────────────────────────────────────────────── */

int main(int argc, char **argv) {
    args_t args;
    if (parse_args(argc, argv, &args) < 0) {
        return 1;
    }

    /* 打印配置 */
    const char *format_name = (args.plot_format == XS_PLOT_FORMAT_XY) ? "xy" : "interleaved";
    double frame_interval = (double)args.samples_per_channel / args.rate;
    double frame_rate = args.rate / (double)args.samples_per_channel;

    printf("═══ pipeview Serial Plot Generator ═══\n");
    printf("串口:    %s @ %d baud\n", args.port, args.baudrate);
    printf("通道:    %u  格式: %s\n", args.channels, format_name);
    printf("频率:    %.1f Hz  振幅: %.1f\n", args.freq, args.amp);
    printf("采样率:  %.0f samples/sec  spc: %u\n", args.rate, args.samples_per_channel);
    printf("帧间隔:  %.3f ms  (%.2f fps)\n", frame_interval * 1000.0, frame_rate);
    printf("文本间隔: %.1f s\n", args.text_interval);
    printf("═══════════════════════════════════════\n");

    /* 分配缓冲 */
    uint32_t value_count = (uint32_t)args.channels * (uint32_t)args.samples_per_channel;
    uint32_t payload_len = value_count * 4;
    size_t   packet_cap = (size_t)XS_PLOT_HEADER_SIZE + (size_t)payload_len;
    size_t   frame_cap = 2u + xs_cobs_max_encoded_size(packet_cap) + 1u;

    uint8_t *packet_buf = malloc(packet_cap);
    uint8_t *frame_buf  = malloc(frame_cap);
    float   *samples    = malloc((size_t)value_count * sizeof(float));

    if (!packet_buf || !frame_buf || !samples) {
        fprintf(stderr, "内存分配失败\n");
        free(packet_buf); free(frame_buf); free(samples);
        return 1;
    }

    /* 打开串口 */
    int fd = serial_open(args.port, args.baudrate);
    if (fd < 0) {
        fprintf(stderr, "无法打开串口 %s\n", args.port);
        free(packet_buf); free(frame_buf); free(samples);
        return 1;
    }
    printf("已打开 %s\n\n", args.port);

    /* 设置信号处理 */
    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_handler = sigint_handler;
    sigaction(SIGINT, &sa, NULL);
    sigaction(SIGTERM, &sa, NULL);

    /* 主循环 */
    uint32_t sample_index = 0;
    uint64_t frame_count  = 0;
    uint64_t text_count   = 0;
    double   started_at   = now_sec();
    double   next_text_at = started_at + args.text_interval;

    printf("开始发送 (Ctrl+C 停止) ...\n");

    while (g_running) {
        /* 生成采样数据 */
        generate_samples(samples, sample_index,
                         args.channels, args.samples_per_channel,
                         args.plot_format, args.rate, args.freq, args.amp);

        /* 发送 plot 帧 */
        if (send_plot_frame(fd, samples,
                            args.channels, args.samples_per_channel,
                            args.plot_format,
                            packet_buf, packet_cap,
                            frame_buf, frame_cap) < 0) {
            fprintf(stderr, "发送失败, 退出\n");
            break;
        }
        frame_count++;

        /* 检查是否需要发送文本行 */
        double now = now_sec();
        if (now >= next_text_at) {
            if (send_text_line(fd, sample_index, samples,
                               args.channels, args.samples_per_channel,
                               format_name) < 0) {
                fprintf(stderr, "发送文本失败, 退出\n");
                break;
            }
            text_count++;
            next_text_at += args.text_interval;
        }

        sample_index += args.samples_per_channel;

        /* 帧间隔时序控制 */
        double elapsed = now_sec() - started_at;
        double target = (double)frame_count * frame_interval;
        double wait = target - elapsed;
        if (wait > 0.0) {
            sleep_sec(wait);
        }
    }

    double total_time = now_sec() - started_at;
    printf("\n");
    printf("═══ 已停止 ═══\n");
    printf("运行时间: %.1f 秒\n", total_time);
    printf("Plot 帧:  %lu  (%.1f fps)\n",
           (unsigned long)frame_count,
           total_time > 0.0 ? (double)frame_count / total_time : 0.0);
    printf("文本行:   %lu\n", (unsigned long)text_count);

    /* 清理 */
    tcflush(fd, TCIOFLUSH);
    close(fd);
    free(packet_buf);
    free(frame_buf);
    free(samples);

    return 0;
}
