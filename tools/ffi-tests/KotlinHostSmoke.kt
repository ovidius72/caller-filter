package ffi.tests

import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.Callable
import uniffi.callerfilter_core.*

fun main() {
    fun fixture(name: String) = File("target/ffi-tests/fixtures/$name").readBytes()
    fun exact(id: ULong, effect: EffectInput, digits: String) = RuleInput(id, effect, MatcherInput.Exact(digits))
    val bytes = fixture("numbering.cfnd")
    Snapshot(bytes, listOf(fixture("places-user.cfds"), fixture("places-en.cfds"))).use { snapshot ->
        check(coreVersion() == "0.1.0")
        check(snapshot.numberingUpstream() == "future-test-version")
        check(snapshot.numberMetadataFormatVersion() == 1.toUShort())
        check(snapshot.datasetFormatVersion() == 1.toUShort())
        check(snapshot.datasetVersions().size == 2)
        check(snapshot.placePrefixes("Test Town", "en") == listOf("3902", "3903"))
        check(snapshot.placeNames("en") == listOf("Test Town"))
        check(normalizeNumber("0200000000", "IT", snapshot).e164 == "+390200000000")
        check(normalizeNumber("+39 0200000000", "absent", snapshot).e164 == "+390200000000")
        check(geocodeNumber("+390200000000", snapshot) == LocatedOutput.Place("Test Town", "en"))
        check(geocodeNumber("+390400000000", snapshot) == LocatedOutput.Place("Test Town", "test-language"))
        check(geocodeNumber("+393000000000", snapshot) == LocatedOutput.NotGeographic)
        check(geocodeNumber("+390500000000", snapshot) == LocatedOutput.NoData)
        check(geocodeNumber("bad", snapshot) == LocatedOutput.NotANumber)
        check(isNumberGeographic("+390200000000", snapshot))
        Snapshot(fixture("empty.cfnd"), emptyList()).use { empty ->
            check(expectError<FfiException.Normalize> { normalizeNumber("0200000000", "IT", empty) }.reason == NormalizeFailure.REGION_NOT_LOADED)
        }
        check(expectError<FfiException.NumberMetadata> { Snapshot("oops".toByteArray(), emptyList()) }.reason == DataFailure.BadMagic)
        check(expectError<FfiException.Dataset> { Snapshot(bytes, listOf("oops".toByteArray())) }.reason == DataFailure.BadMagic)
        check(snapshot.numberingUpstream() == "future-test-version")
        val first = ULong.MAX_VALUE - 1uL
        PreparedRules(listOf(exact(first, EffectInput.DENY, "390200000000"), exact(2uL, EffectInput.DENY, "390200000001"))).use { rules ->
            check(evaluateNumber("+390200000000", null, rules).matchedRule == first)
            check(evaluateNumber("+390200000000", null, rules).decision == DecisionOutput.Block)
            PreparedRules(listOf(exact(1uL, EffectInput.DENY, "123"), exact(2uL, EffectInput.ALLOW, "123"))).use { conflicts ->
                check(conflicts.conflicts() == listOf(ConflictOutput(1uL, 2uL)))
                check(evaluateNumber("123", null, conflicts).contested)
            }
            check(expectError<FfiException.DuplicateRuleId> { PreparedRules(listOf(exact(1uL, EffectInput.DENY, "123"), exact(1uL, EffectInput.DENY, "456"))) }.id == 1uL)
            expectError<FfiException.InvalidNumber> { evaluateNumber("bad", null, rules) }
            check(expectError<FfiException.Rule> { PreparedRules(listOf(RuleInput(1uL, EffectInput.DENY, MatcherInput.Pattern("xxx")))) }.reason == RuleFailure.PatternPinsNothing)
            val budget = BudgetInput(2uL, 10uL)
            val live = SurfaceInput("live", true, true, null)
            val list = SurfaceInput("list", false, false, budget)
            check(explainRule(first, rules, snapshot, listOf(live, list)).verdicts.map { it.verdict } == listOf(ExplainVerdict.AppliesLive, ExplainVerdict.Fits(1uL)))
            val sink = TestSink { check(rules.len() == 2uL); ExpansionStatus.CONTINUE }
            check(expandRulesBatched(rules, snapshot, budget, 1u, sink) == ExpansionOutput.Fits(2uL))
            check(sink.batches == listOf(listOf(390200000000L), listOf(390200000001L)))
            val never = TestSink()
            check(expandRulesBatched(rules, snapshot, BudgetInput(1uL, 10uL), 1u, never) == ExpansionOutput.TooBroad(2uL, true, listOf(2uL, first)))
            check(never.batches.isEmpty())
            val cancel = TestSink { ExpansionStatus.CANCEL }
            check(expandRulesBatched(rules, snapshot, budget, 1u, cancel) == ExpansionOutput.Cancelled(1uL))
            check(cancel.batches.size == 1)
            check(expectError<FfiException.Callback> { expandRulesBatched(rules, snapshot, budget, 1u, TestSink { throw FfiException.Callback("stop") }) }.reason == "stop")
            check(expectError<FfiException.UnexpectedCallback> { expandRulesBatched(rules, snapshot, budget, 1u, TestSink { throw IllegalStateException("foreign failure") }) }.reason.contains("foreign failure"))
            expectError<FfiException.InvalidBatchSize> { expandRulesBatched(rules, snapshot, budget, 0u, never) }
            check(expandRulesBatched(rules, snapshot, budget, UInt.MAX_VALUE, TestSink()) == ExpansionOutput.Fits(2uL))
            PreparedRules(listOf(RuleInput(1uL, EffectInput.DENY, MatcherInput.Pattern("39020000000x")), exact(2uL, EffectInput.ALLOW, "390200000008"))).use { carved ->
                val carvedSink = TestSink()
                check(expandRulesBatched(carved, snapshot, BudgetInput(9uL, 10uL), 2u, carvedSink) == ExpansionOutput.Fits(9uL))
                val numbers = carvedSink.batches.flatten()
                check(numbers.size == 9 && 390200000008L !in numbers)
                check(numbers.zipWithNext().all { (a, b) -> a < b })
            }
            PreparedRules(listOf(RuleInput(3uL, EffectInput.DENY, MatcherInput.Suffix("123")))).use { suffix ->
                check(expandRulesBatched(suffix, snapshot, budget, 1u, never) == ExpansionOutput.NotExpandable(3uL, NotExpandableOutput.SUFFIX))
            }
            prepareRulesForSnapshot(listOf(RuleInput(5uL, EffectInput.DENY, MatcherInput.Location("Test Town", "en"))), snapshot).use { location ->
                check(explainRule(5uL, location, snapshot, listOf(live)).caveats == listOf(CaveatOutput.LANDLINES_ONLY))
                Snapshot(bytes, listOf(fixture("places-new.cfds"))).use { changed ->
                    expectError<FfiException.SnapshotMismatch> { explainRule(5uL, location, changed, listOf(live)) }
                    expectError<FfiException.SnapshotMismatch> { expandRulesBatched(location, changed, budget, 1u, never) }
                }
            }
            val pool = Executors.newFixedThreadPool(4)
            try {
                pool.invokeAll((1..20).map { Callable { check(evaluateNumber("+390200000000", null, rules).decision == DecisionOutput.Block) } }).forEach { it.get() }
            } finally { pool.shutdown() }
        }
    }
    println("Kotlin FFI: data, rules, errors, snapshots, callbacks, cancellation, reentry and threads passed")
}
