package com.antoniopantano.callerfilter

import java.io.StringReader
import java.security.MessageDigest
import java.util.Properties
import uniffi.callerfilter_core.*

/** A private, prevalidated exact fixture, not a production rule authoring format. */
data class ProbeFixture(val alias: String, val number: String?, val identity: String) {
    companion object {
        fun parse(bytes: ByteArray): ProbeFixture {
            val properties = object : Properties() {
                override fun put(key: Any, value: Any): Any? {
                    require(!containsKey(key)) { "Duplicate fixture field" }
                    return super.put(key, value)
                }
            }.apply { load(StringReader(bytes.toString(Charsets.UTF_8))) }
            require(properties.stringPropertyNames().all { it in setOf("version", "alias", "mode", "number") })
            require(properties.getProperty("version") == "1")
            val alias = properties.getProperty("alias") ?: error("Missing fixture alias")
            require(alias.matches(Regex("[A-Za-z0-9_-]+")))
            val number = when (properties.getProperty("mode")) {
                "empty" -> { require(!properties.containsKey("number")); null }
                "exact" -> properties.getProperty("number").also { require(it != null && it.startsWith("+")) }
                else -> error("Unsupported fixture mode")
            }
            val identity = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
            return ProbeFixture(alias, number, identity)
        }
    }
}

enum class ProbeReason {
    DENY, EMPTY, NONMATCH, UNSUPPORTED_HANDLE, CORE_UNAVAILABLE, UNEXPECTED_VERDICT,
    DEADLINE, ROLE_UNAVAILABLE, OUTGOING, UNBOUND, BAD_CONFIGURATION,
}

data class ProbeDecision(val reason: ProbeReason, val matchedRule: ULong? = null) {
    // OS response mapping only. Which number matches is decided by Rust.
    val flags: ProbeResponseFlags get() {
        val blocked = reason == ProbeReason.DENY
        return ProbeResponseFlags(blocked, blocked, blocked)
    }
}

data class ProbeResponseFlags(val disallow: Boolean, val reject: Boolean, val skipNotification: Boolean)

/** Access on the store's serial executor; never close a handle during evaluation. */
class ProbeEngine(val fixture: ProbeFixture) : AutoCloseable {
    private val deny: PreparedRules? = fixture.number?.let {
        // Remove only the wire-format marker; do not normalize or implement matching in Kotlin.
        PreparedRules(listOf(RuleInput(1uL, EffectInput.DENY, MatcherInput.Exact(it.removePrefix("+")))))
    }
    private val empty = PreparedRules(emptyList())
    var blocking = false
        private set

    fun setBlocking(value: Boolean) {
        require(!value || deny != null)
        blocking = value
    }

    fun evaluate(number: String?): ProbeDecision {
        // This minimal probe has no national-input context. Do not guess a country.
        if (number == null || !number.startsWith("+")) return ProbeDecision(ProbeReason.UNSUPPORTED_HANDLE)
        val verdict = evaluateNumber(number, null, if (blocking) deny!! else empty)
        if (verdict.contested) return ProbeDecision(ProbeReason.UNEXPECTED_VERDICT)
        return when (verdict.decision) {
            DecisionOutput.Block -> ProbeDecision(ProbeReason.DENY, verdict.matchedRule)
            DecisionOutput.Allow -> ProbeDecision(if (blocking) ProbeReason.NONMATCH else ProbeReason.EMPTY)
            else -> ProbeDecision(ProbeReason.UNEXPECTED_VERDICT)
        }
    }

    override fun close() { deny?.close(); empty.close() }
}

/** At most one response, even when native work completes after the local deadline. */
class ProbeResponseGate(private val deadlineNanos: Long) {
    private var finished = false
    @Synchronized fun claim(nowNanos: Long, decision: ProbeDecision): ProbeDecision? {
        if (finished) return null
        finished = true
        return if (nowNanos >= deadlineNanos) ProbeDecision(ProbeReason.DEADLINE) else decision
    }
    @Synchronized fun cancel(): Boolean {
        if (finished) return false
        finished = true
        return true
    }
}
