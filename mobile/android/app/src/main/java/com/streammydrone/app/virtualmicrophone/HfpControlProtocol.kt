package com.streammydrone.app.virtualmicrophone

/** Strict, bounded parsing. No received text is ever added to the diagnostic report. */
internal object HfpControlProtocol {
    private val features = Regex("\\+BRSF:\\s*([0-9]{1,10})")
    private val indicator = Regex("\\(\\s*\"([A-Za-z][A-Za-z0-9_]{0,31})\"\\s*,\\s*\\(([0-9,\\-\\s]{1,80})\\)\\s*\\)")
    private val mandatory = mapOf(
        "service" to (0..1).toSet(), "call" to (0..1).toSet(),
        "callsetup" to (0..3).toSet(), "callheld" to (0..2).toSet(),
        "signal" to (0..5).toSet(), "roam" to (0..1).toSet(),
        "battchg" to (0..5).toSet(),
    )

    fun features(line: String): Long? = features.matchEntire(line)?.groupValues?.get(1)
        ?.toLongOrNull()?.takeIf { it <= 0xffff_ffffL }

    fun indicators(line: String): List<Set<Int>>? {
        if (!line.startsWith("+CIND:")) return null
        val body = line.substringAfter(':').trim()
        val matches = indicator.findAll(body).toList()
        if (matches.size !in 7..32) return null
        var end = 0
        val seen = mutableSetOf<String>()
        val result = mutableListOf<Set<Int>>()
        for ((index, match) in matches.withIndex()) {
            if (body.substring(end, match.range.first).trim() != if (index == 0) "" else ",") return null
            end = match.range.last + 1
            val name = match.groupValues[1]
            if (!seen.add(name)) return null
            val values = mutableSetOf<Int>()
            for (part in match.groupValues[2].split(',')) {
                val range = part.trim().split('-')
                if (range.size !in 1..2) return null
                val first = range[0].trim().toIntOrNull()?.takeIf { it in 0..255 } ?: return null
                val last = if (range.size == 2) range[1].trim().toIntOrNull() ?: return null else first
                if (last !in first..255) return null
                values.addAll(first..last)
            }
            if (mandatory[name]?.let { it != values } == true) return null
            result += values
        }
        if (body.substring(end).isNotBlank() || !seen.containsAll(mandatory.keys)) return null
        return result
    }

    fun statuses(line: String, indicators: List<Set<Int>>): Boolean {
        if (!line.startsWith("+CIND:")) return false
        val tokens = line.substringAfter(':').split(',')
        return tokens.size == indicators.size && tokens.indices.all { index ->
            tokens[index].trim().toIntOrNull()?.let { it in indicators[index] } == true
        }
    }

    /** Runs on-device before any Bluetooth work; covers malformed and deceptive success input. */
    fun selfCheck() {
        val sample = "+CIND: (\"service\",(0,1)),(\"call\",(0-1)),(\"callsetup\",(0-3))," +
            "(\"callheld\",(0-2)),(\"signal\",(0-5)),(\"roam\",(0-1)),(\"battchg\",(0-5))"
        val parsed = checkNotNull(indicators(sample))
        check(features("+BRSF: 0") == 0L)
        check(features("+BRSF: 4294967295") == 0xffff_ffffL)
        check(features("+BRSF: 4294967296") == null)
        check(features("+BRSF: -1") == null)
        check(features("+BRSF: 0 unexpected") == null)
        check(statuses("+CIND: 1,0,0,0,5,0,5", parsed))
        check(!statuses("+CIND: 1,0,0,0,6,0,5", parsed))
        check(!statuses("+CIND: 1,0", parsed))
        check(!statuses("+CIND: 1,0,0,0,5,0,5,0", parsed))
        check(indicators(sample.replace("(0-3)", "(0-2)")) == null)
        check(indicators(sample.replace("\"callsetup\"", "\"call\"")) == null)
        check(indicators(sample + "garbage") == null)
        check(indicators(sample.replace(",(\"call\"", "(\"call\"")) == null)
        check(indicators(sample.replace("(0-5)", "(5-0)")) == null)
    }
}
