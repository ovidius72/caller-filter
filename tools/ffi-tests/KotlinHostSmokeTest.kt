package ffi.tests

import uniffi.callerfilter_core.*

inline fun <reified T : Throwable> expectError(body: () -> Unit): T {
    try { body() } catch (error: Throwable) {
        check(error is T) { "expected ${T::class}, got $error" }
        return error
    }
    error("expected ${T::class}")
}

class TestSink(private val action: () -> ExpansionStatus = { ExpansionStatus.CONTINUE }) : ExpansionSink {
    val batches = mutableListOf<List<Long>>()
    override fun onBatch(values: List<Long>): ExpansionStatus {
        batches.add(values)
        return action()
    }
}
