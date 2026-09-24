package com.djilivebridge.android

import java.io.ByteArrayOutputStream

// FLV tag bodies for H.264 + AAC, the stream DJI Fly itself sends over RTMP.

private const val NAL_TYPE_SPS = 7
private const val NAL_TYPE_PPS = 8

/**
 * True when [length] bytes of [data] start with a 4-byte Annex-B start code, which is what
 * MediaExtractor writes. A 3-byte code is not accepted here: a length-prefixed NAL unit of 256 to
 * 511 bytes starts with the same bytes.
 */
internal fun isAnnexB(data: ByteArray, length: Int = data.size): Boolean =
    length >= 4 && data[0].toInt() == 0 && data[1].toInt() == 0 && data[2].toInt() == 0 && data[3].toInt() == 1

/** Splits an Annex-B stream into NAL units without their start codes. */
internal fun annexBNalUnits(data: ByteArray, length: Int = data.size): List<ByteArray> {
    val units = mutableListOf<ByteArray>()
    var start = -1
    var index = 0
    while (index + 2 < length) {
        if (data[index].toInt() == 0 && data[index + 1].toInt() == 0 && data[index + 2].toInt() == 1) {
            if (start >= 0) units += data.copyOfRange(start, trimTrailingZeros(data, start, index))
            index += 3
            start = index
        } else {
            index++
        }
    }
    if (start >= 0 && start < length) units += data.copyOfRange(start, trimTrailingZeros(data, start, length))
    return units.filter { it.isNotEmpty() }
}

// The zero before a 4-byte start code (and any trailing_zero_8bits) is not part of the NAL unit.
private fun trimTrailingZeros(data: ByteArray, start: Int, end: Int): Int {
    var last = end
    while (last > start && data[last - 1].toInt() == 0) last--
    return last
}

/** One access unit in the 4-byte length-prefixed (AVCC) form FLV carries. */
internal fun toAvcc(data: ByteArray, length: Int): ByteArray {
    if (!isAnnexB(data, length)) return data.copyOf(length)
    val out = ByteArrayOutputStream(length + 16)
    annexBNalUnits(data, length).forEach { unit ->
        out.write(unit.size ushr 24 and 0xFF)
        out.write(unit.size ushr 16 and 0xFF)
        out.write(unit.size ushr 8 and 0xFF)
        out.write(unit.size and 0xFF)
        out.write(unit)
    }
    return out.toByteArray()
}

/**
 * The AVCDecoderConfigurationRecord for the codec data a demuxer hands out: either SPS and PPS
 * (with or without start codes, in any order) or a complete avcC record. Null when either
 * parameter set is missing.
 */
internal fun avcConfigurationRecord(vararg codecData: ByteArray): ByteArray? {
    codecData.singleOrNull()?.let { single -> if (single.size > 6 && single[0].toInt() == 1) return single }
    val units = codecData.flatMap { if (isAnnexB(it)) annexBNalUnits(it) else listOf(it) }
    val sps = units.firstOrNull { it.nalType() == NAL_TYPE_SPS && it.size >= 4 } ?: return null
    val pps = units.firstOrNull { it.nalType() == NAL_TYPE_PPS } ?: return null
    return ByteArrayOutputStream().apply {
        write(1)
        write(sps[1].toInt()) // profile_idc
        write(sps[2].toInt()) // constraint flags
        write(sps[3].toInt()) // level_idc
        write(0xFF) // 4-byte NAL length prefixes
        write(0xE1) // one SPS
        write(sps.size ushr 8 and 0xFF)
        write(sps.size and 0xFF)
        write(sps)
        write(1) // one PPS
        write(pps.size ushr 8 and 0xFF)
        write(pps.size and 0xFF)
        write(pps)
    }.toByteArray()
}

private fun ByteArray.nalType(): Int = if (isEmpty()) -1 else this[0].toInt() and 0x1F

internal fun flvAvcSequenceHeader(configurationRecord: ByteArray): ByteArray =
    byteArrayOf(0x17, 0x00, 0x00, 0x00, 0x00) + configurationRecord

/** A video tag body: frame type + codec, NALU packet, 24-bit composition time, AVCC data. */
internal fun flvAvcFrame(avcc: ByteArray, keyframe: Boolean, compositionTimeMs: Int): ByteArray =
    byteArrayOf(
        if (keyframe) 0x17 else 0x27,
        0x01,
        (compositionTimeMs ushr 16).toByte(),
        (compositionTimeMs ushr 8).toByte(),
        compositionTimeMs.toByte(),
    ) + avcc

// 0xAF: AAC; the rate/size/channel bits are fixed for AAC and the decoder uses the ASC instead.
internal fun flvAacSequenceHeader(audioSpecificConfig: ByteArray): ByteArray =
    byteArrayOf(0xAF.toByte(), 0x00) + audioSpecificConfig

internal fun flvAacFrame(data: ByteArray, length: Int): ByteArray =
    byteArrayOf(0xAF.toByte(), 0x01) + data.copyOf(length)

/**
 * Decode timestamps for video samples given in decode order. Demuxers only report presentation
 * times; with B-frames those are out of order, so the decode times are the sorted presentation
 * times, delayed just enough that no frame is decoded after it is shown.
 */
internal fun decodeTimestamps(presentationTimesUs: LongArray): LongArray {
    val sorted = presentationTimesUs.sortedArray()
    var delay = 0L
    for (index in sorted.indices) delay = maxOf(delay, sorted[index] - presentationTimesUs[index])
    return LongArray(sorted.size) { sorted[it] - delay }
}
