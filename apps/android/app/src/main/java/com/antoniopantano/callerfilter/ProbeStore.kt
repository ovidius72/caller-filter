package com.antoniopantano.callerfilter

import android.app.role.RoleManager
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import java.util.concurrent.Executors
import uniffi.callerfilter_core.coreVersion
import uniffi.callerfilter_core.FfiException

enum class ProbeRole { HELD, NOT_GRANTED, UNAVAILABLE, ERROR }

fun probeRole(context: Context): ProbeRole {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return ProbeRole.UNAVAILABLE
    return try {
        val manager = context.getSystemService(RoleManager::class.java)
        when {
            manager == null || !manager.isRoleAvailable(RoleManager.ROLE_CALL_SCREENING) -> ProbeRole.UNAVAILABLE
            manager.isRoleHeld(RoleManager.ROLE_CALL_SCREENING) -> ProbeRole.HELD
            else -> ProbeRole.NOT_GRANTED
        }
    } catch (_: RuntimeException) { ProbeRole.ERROR }
}

data class ProbeSnapshot(
    val ready: Boolean, val alias: String, val hasDeny: Boolean,
    val blocking: Boolean, val core: String, val error: String?,
)

/** All native handles and preference changes are confined to one worker thread. */
object ProbeStore {
    private val worker = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    private var initialized = false
    private var engine: ProbeEngine? = null
    private var core = "unavailable"
    private var failure: String? = null

    private fun initialize(context: Context) {
        if (initialized) return
        initialized = true
        try {
            val fixture = context.assets.open("probe.properties").use { ProbeFixture.parse(it.readBytes()) }
            val prepared = ProbeEngine(fixture)
            engine = prepared
            core = coreVersion()
            val prefs = context.getSharedPreferences("probe", Context.MODE_PRIVATE)
            // Updating the packaged fixture must never silently activate a different number.
            prepared.setBlocking(fixture.number != null && prefs.getString("active-fixture", null) == fixture.identity)
        } catch (error: Exception) { fail(error.javaClass.simpleName) }
        catch (error: LinkageError) { fail(error.javaClass.simpleName) }
    }

    private fun fail(kind: String) {
        failure = "Setup failed ($kind). Calls will be allowed; this is not a valid blocking trial."
    }

    private fun snapshot() = ProbeSnapshot(
        ready = engine != null && failure == null,
        alias = engine?.fixture?.alias ?: "unavailable",
        hasDeny = engine?.fixture?.number != null,
        blocking = engine?.blocking == true && failure == null,
        core = core, error = failure,
    )

    fun refresh(context: Context, callback: (ProbeSnapshot) -> Unit) {
        val app = context.applicationContext
        worker.execute {
            initialize(app)
            val value = snapshot()
            main.post { callback(value) }
        }
    }

    fun selectBlocking(context: Context, enabled: Boolean, callback: (ProbeSnapshot) -> Unit) {
        val app = context.applicationContext
        worker.execute {
            initialize(app)
            try {
                val current = engine
                require(!enabled || (current != null && current.fixture.number != null && failure == null))
                // Clear in-memory blocking even if persistence fails; never keep silently denying.
                if (!enabled) current?.setBlocking(false)
                val edit = app.getSharedPreferences("probe", Context.MODE_PRIVATE).edit()
                if (enabled) edit.putString("active-fixture", current!!.fixture.identity)
                else edit.remove("active-fixture")
                check(edit.commit()) { "Preference write failed" }
                current?.setBlocking(enabled)
            } catch (error: Exception) { fail(error.javaClass.simpleName) }
            val value = snapshot()
            main.post { callback(value) }
        }
    }

    fun evaluate(context: Context, number: String?, callback: (ProbeDecision) -> Unit) {
        val app = context.applicationContext
        worker.execute {
            initialize(app)
            val decision = try {
                if (failure != null) ProbeDecision(ProbeReason.CORE_UNAVAILABLE)
                else engine?.evaluate(number) ?: ProbeDecision(ProbeReason.CORE_UNAVAILABLE)
            } catch (_: FfiException.InvalidNumber) { ProbeDecision(ProbeReason.UNSUPPORTED_HANDLE) }
            catch (error: Exception) {
                fail(error.javaClass.simpleName)
                ProbeDecision(ProbeReason.CORE_UNAVAILABLE)
            } catch (error: LinkageError) {
                fail(error.javaClass.simpleName)
                ProbeDecision(ProbeReason.CORE_UNAVAILABLE)
            }
            main.post { callback(decision) }
        }
    }
}
