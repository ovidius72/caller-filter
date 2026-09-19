package com.antoniopantano.callerfilter

import android.os.SystemClock
import android.util.Log
import org.json.JSONObject
import java.time.Instant

/** Redacted screening requests, NOT proof that the OS blocked a call. */
object ProbeDiagnostics {
    const val TAG = "CallerFilterProbe"
    @Volatile var latest = "No screening evidence in this process. Capture logcat before trials."
        private set

    fun record(event: String, fields: Map<String, Any> = emptyMap()) {
        val row = JSONObject()
            .put("event", event)
            .put("utc", Instant.now().toString())
            .put("elapsed_ns", SystemClock.elapsedRealtimeNanos())
        fields.forEach { (key, value) -> row.put(key, value) }
        latest = row.toString()
        Log.i(TAG, latest)
    }
}
