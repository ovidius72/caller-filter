package com.antoniopantano.callerfilter

import java.math.BigInteger
import java.io.File
import uniffi.callerfilter_core.FfiException

fun main(args: Array<String>) {
    val seed = args[0]
    val number = "+" + seed
    val other = "+" + (BigInteger(seed) + BigInteger.ONE)
    fun fixture(mode: String, value: String? = null) = ProbeFixture.parse(
        ("version=1\nalias=host-test\nmode=$mode\n" + (value?.let { "number=$it\n" } ?: "")).toByteArray())
    fun fails(body: () -> Unit) {
        try { body() } catch (_: Exception) { return }
        error("Expected fixture rejection")
    }
    val empty = fixture("empty")
    val exact = fixture("exact", number)
    check(empty.identity != exact.identity)
    check(fixture("exact", other).identity != exact.identity)
    fails { fixture("unknown") }
    fails { fixture("empty", number) }
    fails { fixture("exact") }
    fails { fixture("exact", seed) }
    fails { ProbeFixture.parse("version=2\nalias=test\nmode=empty".toByteArray()) }
    fails { ProbeFixture.parse("version=1\nalias=bad alias\nmode=empty".toByteArray()) }
    fails { ProbeFixture.parse("version=1\nalias=test\nmode=empty\nextra=x".toByteArray()) }
    fails { ProbeFixture.parse("version=1\nalias=test\nmode=empty\nmode=exact".toByteArray()) }
    for (bad in listOf("+", "+invalid", "+" + seed + "x")) {
        fails { ProbeEngine(fixture("exact", bad)).close() }
    }
    ProbeEngine(empty).use { engine ->
        check(engine.evaluate(number).reason == ProbeReason.EMPTY)
        check(!engine.evaluate(number).flags.disallow)
        fails { engine.setBlocking(true) }
    }
    ProbeEngine(exact).use { engine ->
        check(engine.evaluate(number).reason == ProbeReason.EMPTY) // Activation is explicit.
        engine.setBlocking(true)
        val deny = engine.evaluate(number)
        check(deny.reason == ProbeReason.DENY && deny.matchedRule == 1uL)
        check(deny.flags == ProbeResponseFlags(true, true, true))
        check(engine.evaluate(other).reason == ProbeReason.NONMATCH)
        check(engine.evaluate(null).reason == ProbeReason.UNSUPPORTED_HANDLE)
        check(engine.evaluate(seed).reason == ProbeReason.UNSUPPORTED_HANDLE)
        try { engine.evaluate("+invalid"); error("Invalid number accepted") }
        catch (_: FfiException.InvalidNumber) { }
        engine.setBlocking(false)
        check(engine.evaluate(number).reason == ProbeReason.EMPTY)
    }
    for (reason in ProbeReason.entries.filter { it != ProbeReason.DENY }) {
        check(ProbeDecision(reason).flags == ProbeResponseFlags(false, false, false))
    }
    // Synthetic clock ticks: exactly-at-deadline, late native completion, cancellation,
    // repeated callbacks and independent requests on a reused service binding.
    val deny = ProbeDecision(ProbeReason.DENY)
    val early = ProbeResponseGate(100)
    check(early.claim(99, deny) == deny)
    check(early.claim(99, deny) == null && !early.cancel())
    val expired = ProbeResponseGate(100)
    check(expired.claim(100, deny)?.reason == ProbeReason.DEADLINE)
    check(expired.claim(101, deny) == null)
    val cancelled = ProbeResponseGate(100)
    check(cancelled.cancel() && !cancelled.cancel())
    check(cancelled.claim(1, deny) == null)
    val next = ProbeResponseGate(200)
    check(next.claim(150, deny) == deny)
    // Validate the actual private build fixture through Rust too. This checks
    // transport shape/matching, not assignment/reachability or numbering plans.
    val packaged = ProbeFixture.parse(File(args[1]).readBytes())
    ProbeEngine(packaged).use { engine ->
        packaged.number?.let {
            engine.setBlocking(true)
            check(engine.evaluate(it).reason == ProbeReason.DENY)
        } ?: check(engine.evaluate(number).reason == ProbeReason.EMPTY)
    }
    println("PASS Android probe host: Rust exact/empty/nonmatch/recovery, fixture errors, flags, deadlines, cancellation and packaged input")
}
