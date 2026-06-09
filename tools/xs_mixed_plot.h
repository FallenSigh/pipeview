#ifndef XS_MIXED_PLOT_H
#define XS_MIXED_PLOT_H

#include <stddef.h>
#include <stdint.h>

/* MixedTextPlot plot frame:
 *   0x1E 'P' + COBS(packet) + 0x00
 *
 * Plot packet:
 *   "XP" +
 *   version:u8 +
 *   format:u8 +
 *   sample_type:u8 +
 *   endian:u8 +
 *   channels:u8 +
 *   samples_per_channel:u16le +
 *   payload_len:u32le +
 *   payload
 */

#define XS_MIXED_PLOT_ESCAPE 0x1E
#define XS_MIXED_PLOT_MARKER 0x50 /* 'P' */

#define XS_PLOT_MAGIC_0 0x58 /* 'X' */
#define XS_PLOT_MAGIC_1 0x50 /* 'P' */
#define XS_PLOT_VERSION 1

#define XS_PLOT_FORMAT_INTERLEAVED 0
#define XS_PLOT_FORMAT_BLOCK 1
#define XS_PLOT_FORMAT_XY 2

#define XS_SAMPLE_TYPE_F32 8
#define XS_ENDIAN_LITTLE 0

#define XS_PLOT_HEADER_SIZE 13

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    XS_OK = 0,
    XS_ERR_ARG = -1,
    XS_ERR_BUF = -2
} xs_status_t;

static size_t xs_cobs_max_encoded_size(size_t input_len) {
    return input_len + (input_len / 254u) + 1u;
}

static void xs_write_u16_le(uint8_t *dst, uint16_t v) {
    dst[0] = (uint8_t)(v & 0xFFu);
    dst[1] = (uint8_t)((v >> 8) & 0xFFu);
}

static void xs_write_u32_le(uint8_t *dst, uint32_t v) {
    dst[0] = (uint8_t)(v & 0xFFu);
    dst[1] = (uint8_t)((v >> 8) & 0xFFu);
    dst[2] = (uint8_t)((v >> 16) & 0xFFu);
    dst[3] = (uint8_t)((v >> 24) & 0xFFu);
}

static void xs_write_f32_le(uint8_t *dst, float v) {
    union {
        float f;
        uint8_t b[4];
    } u;
    u.f = v;

#if defined(__BYTE_ORDER__) && (__BYTE_ORDER__ == __ORDER_BIG_ENDIAN__)
    dst[0] = u.b[3];
    dst[1] = u.b[2];
    dst[2] = u.b[1];
    dst[3] = u.b[0];
#else
    dst[0] = u.b[0];
    dst[1] = u.b[1];
    dst[2] = u.b[2];
    dst[3] = u.b[3];
#endif
}

/* output must have at least xs_cobs_max_encoded_size(input_len) bytes */
static size_t xs_cobs_encode(const uint8_t *input, size_t input_len, uint8_t *output) {
    size_t read_index = 0;
    size_t write_index = 1;
    size_t code_index = 0;
    uint8_t code = 1;

    while (read_index < input_len) {
        if (input[read_index] == 0) {
            output[code_index] = code;
            code = 1;
            code_index = write_index++;
            read_index++;
        } else {
            output[write_index++] = input[read_index++];
            code++;
            if (code == 0xFFu) {
                output[code_index] = code;
                code = 1;
                code_index = write_index++;
            }
        }
    }

    output[code_index] = code;
    return write_index;
}

static xs_status_t xs_plot_build_packet_f32(
    const float *samples,
    uint8_t channels,
    uint16_t samples_per_channel,
    uint8_t plot_format,
    uint8_t *out,
    size_t out_cap,
    size_t *out_len
) {
    size_t i;
    uint32_t value_count;
    uint32_t payload_len;
    size_t packet_len;
    uint8_t *p;

    if (!samples || !out || !out_len) {
        return XS_ERR_ARG;
    }
    if (channels == 0) {
        return XS_ERR_ARG;
    }
    if (plot_format == XS_PLOT_FORMAT_XY && channels != 2) {
        return XS_ERR_ARG;
    }
    if (plot_format > XS_PLOT_FORMAT_XY) {
        return XS_ERR_ARG;
    }

    value_count = (uint32_t)channels * (uint32_t)samples_per_channel;
    payload_len = value_count * 4u;
    packet_len = XS_PLOT_HEADER_SIZE + (size_t)payload_len;

    if (out_cap < packet_len) {
        return XS_ERR_BUF;
    }

    out[0] = XS_PLOT_MAGIC_0;
    out[1] = XS_PLOT_MAGIC_1;
    out[2] = XS_PLOT_VERSION;
    out[3] = plot_format;
    out[4] = XS_SAMPLE_TYPE_F32;
    out[5] = XS_ENDIAN_LITTLE;
    out[6] = channels;
    xs_write_u16_le(&out[7], samples_per_channel);
    xs_write_u32_le(&out[9], payload_len);

    p = &out[13];
    for (i = 0; i < (size_t)value_count; ++i) {
        xs_write_f32_le(p, samples[i]);
        p += 4;
    }

    *out_len = packet_len;
    return XS_OK;
}

static xs_status_t xs_mixed_build_plot_frame_from_packet(
    const uint8_t *packet,
    size_t packet_len,
    uint8_t *out,
    size_t out_cap,
    size_t *out_len
) {
    size_t encoded_len;

    if (!packet || !out || !out_len) {
        return XS_ERR_ARG;
    }
    if (out_cap < 2u + xs_cobs_max_encoded_size(packet_len) + 1u) {
        return XS_ERR_BUF;
    }

    out[0] = XS_MIXED_PLOT_ESCAPE;
    out[1] = XS_MIXED_PLOT_MARKER;
    encoded_len = xs_cobs_encode(packet, packet_len, &out[2]);
    out[2 + encoded_len] = 0x00;

    *out_len = 2u + encoded_len + 1u;
    return XS_OK;
}

static xs_status_t xs_mixed_build_plot_frame_f32(
    const float *samples,
    uint8_t channels,
    uint16_t samples_per_channel,
    uint8_t plot_format,
    uint8_t *packet_buf,
    size_t packet_buf_cap,
    uint8_t *frame_buf,
    size_t frame_buf_cap,
    size_t *frame_len
) {
    xs_status_t rc;
    size_t packet_len = 0;

    if (!packet_buf || !frame_buf || !frame_len) {
        return XS_ERR_ARG;
    }

    rc = xs_plot_build_packet_f32(
        samples,
        channels,
        samples_per_channel,
        plot_format,
        packet_buf,
        packet_buf_cap,
        &packet_len
    );
    if (rc != XS_OK) {
        return rc;
    }

    return xs_mixed_build_plot_frame_from_packet(
        packet_buf,
        packet_len,
        frame_buf,
        frame_buf_cap,
        frame_len
    );
}

#ifdef __cplusplus
}
#endif

#endif /* XS_MIXED_PLOT_H */
