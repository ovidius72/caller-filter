package com.antoniopantano.callerfilter

import android.content.Intent
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.SystemClock
import android.telecom.Call
import android.telecom.CallScreeningService
import java.util.UUID

class ScreeningService : CallScreeningService() {
    private val main = Handler(Looper.getMainLooper())
    private var boundAt: Long? = null
    private var firstRequestAfterBind = false
    private val pending = mutableMapOf<String, Pair<ProbeResponseGate, Runnable>>()

    override fun onCreate() {
        super.onCreate()
        ProbeDiagnostics.record("service_create")
        // Asset/preferences/native initialization runs off the screening callback thread.
        ProbeStore.refresh(this) { /* onScreenCall uses the same serialized worker. */ }
    }

    override fun onBind(intent: Intent): IBinder? {
        boundAt = SystemClock.elapsedRealtimeNanos()
        firstRequestAfterBind = true
        ProbeDiagnostics.record("service_bind")
        return super.onBind(intent)
    }

    override fun onScreenCall(callDetails: Call.Details) {
        val entered = SystemClock.elapsedRealtimeNanos()
        val id = UUID.randomUUID().toString()
        // A reused binding is not a new per-call bind timestamp. Report that gap.
        val requestBind = if (firstRequestAfterBind) boundAt else null
        firstRequestAfterBind = false
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && callDetails.callDirection == Call.Details.DIRECTION_OUTGOING) {
            ProbeDiagnostics.record("screening", mapOf("request" to id, "reason" to "OUTGOING", "response" to "not_required"))
            return
        }
        val budgetMs = resources.getInteger(R.integer.probe_response_budget_ms)
        // Five seconds is the verified SDK contract, not a measured device threshold.
        val validBudget = budgetMs in 1 until 5_000
        val deadline = (requestBind ?: entered) + (if (validBudget) budgetMs else 0).toLong() * 1_000_000
        val gate = ProbeResponseGate(if (validBudget) deadline else Long.MAX_VALUE)
        fun complete(decision: ProbeDecision) = submit(id, callDetails, entered, requestBind, gate, decision)
        val timeout = Runnable { complete(ProbeDecision(ProbeReason.DEADLINE)) }
        pending[id] = gate to timeout
        val remaining = (deadline - SystemClock.elapsedRealtimeNanos()).coerceAtLeast(0)
        if (validBudget) main.postDelayed(timeout, (remaining + 999_999) / 1_000_000)
        ProbeDiagnostics.record("screening_enter", mapOf(
            "request" to id, "local_budget_ms" to budgetMs,
            "bind_observed_for_request" to (requestBind != null),
            "handle" to if (callDetails.handle?.scheme == "tel") "tel" else "unsupported",
        ))
        when {
            !validBudget -> complete(ProbeDecision(ProbeReason.BAD_CONFIGURATION))
            probeRole(this) != ProbeRole.HELD -> complete(ProbeDecision(ProbeReason.ROLE_UNAVAILABLE))
            callDetails.handle?.scheme != "tel" -> complete(ProbeDecision(ProbeReason.UNSUPPORTED_HANDLE))
            else -> ProbeStore.evaluate(this, callDetails.handle?.schemeSpecificPart) { decision -> complete(decision) }
        }
    }

    private fun submit(id: String, details: Call.Details, entered: Long, requestBind: Long?, gate: ProbeResponseGate, candidate: ProbeDecision) {
        val now = SystemClock.elapsedRealtimeNanos()
        val decision = gate.claim(now, candidate) ?: return
        pending.remove(id)?.let { main.removeCallbacks(it.second) }
        val flags = decision.flags
        val response = CallResponse.Builder()
            .setDisallowCall(flags.disallow)
            // Disconnect as if manually rejected, rather than only silencing a visible call.
            .setRejectCall(flags.reject)
            .setSkipNotification(flags.skipNotification)
            .setSkipCallLog(false) // Ordinary third-party apps cannot control this anyway.
            .build()
        val apiResult = try {
            respondToCall(details, response)
            "api_returned" // SDK swallows RemoteException; this is NOT an OS acknowledgement.
        } catch (_: RuntimeException) { "api_error" }
        val finished = SystemClock.elapsedRealtimeNanos()
        ProbeDiagnostics.record("screening_response", mapOf(
            "request" to id, "reason" to decision.reason.name, "response" to apiResult,
            "matched_rule" to (decision.matchedRule?.toString() ?: "none"),
            "disallow" to flags.disallow, "reject" to flags.reject,
            "skip_notification" to flags.skipNotification, "skip_call_log" to false,
            "silence_only" to false, "callback_to_return_ms" to (finished - entered) / 1_000_000.0,
            "local_bind_to_return_ms" to (requestBind?.let { (finished - it) / 1_000_000.0 } ?: -1.0),
            "framework_acceptance" to "unverified",
        ))
    }

    private fun abandonPending() {
        pending.forEach { (id, entry) ->
            main.removeCallbacks(entry.second)
            if (entry.first.cancel()) ProbeDiagnostics.record("screening_abandoned", mapOf("request" to id, "reason" to "UNBOUND"))
        }
        pending.clear()
    }

    override fun onUnbind(intent: Intent): Boolean {
        abandonPending()
        boundAt = null
        ProbeDiagnostics.record("service_unbind")
        return super.onUnbind(intent)
    }

    override fun onDestroy() {
        abandonPending()
        super.onDestroy()
    }
}
