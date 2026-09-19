package com.antoniopantano.callerfilter

import android.app.role.RoleManager
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : AppCompatActivity() {
    private lateinit var status: TextView
    private lateinit var diagnostics: TextView
    private lateinit var roleButton: Button
    private lateinit var denyButton: Button
    private lateinit var emptyButton: Button
    private var state: ProbeSnapshot? = null
    private val roleRequest = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { refresh() }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val layout = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        val scroll = ScrollView(this).apply { addView(layout) }
        fun text(value: String) = TextView(this).apply {
            text = value; textSize = 18f; setPadding(0, 12, 0, 12)
            layout.addView(this)
        }
        fun button(label: String, action: () -> Unit) = Button(this).apply {
            text = label; setOnClickListener { action() }; layout.addView(this)
        }
        ViewCompat.setAccessibilityHeading(text("Android silent-blocking probe"), true)
        status = text("Loading private fixture and Rust core…")
        roleButton = button("Grant call-screening role") {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                try {
                    roleRequest.launch(getSystemService(RoleManager::class.java).createRequestRoleIntent(RoleManager.ROLE_CALL_SCREENING))
                } catch (_: RuntimeException) { status.text = "Cannot request screening role. Check system settings." }
            }
        }
        denyButton = button("Activate bundled exact deny") { select(true) }
        emptyButton = button("Use empty control / clear deny") { select(false) }
        button("Refresh state and diagnostics") { refresh() }
        button("Open default-app settings") {
            try { startActivity(Intent(Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS)) }
            catch (_: RuntimeException) { status.text = "Open default-app settings manually to revoke the screening role." }
        }
        text("One prevalidated number from private build data. No number editor or country guessing. Denied calls request rejection and missed-call notification suppression. Call records may remain.")
        text("Not a blocked-call history. Diagnostics show screening requests and response attempts, not what the phone displayed. Capture logs before the trial.")
        diagnostics = text(ProbeDiagnostics.latest).apply { setTextIsSelectable(true); textSize = 13f }
        val padding = (16 * resources.displayMetrics.density).toInt()
        ViewCompat.setOnApplyWindowInsetsListener(scroll) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            view.setPadding(padding + bars.left, padding + bars.top, padding + bars.right, padding + bars.bottom)
            insets
        }
        setContentView(scroll)
        render()
    }

    override fun onResume() { super.onResume(); refresh() }

    private fun refresh() {
        render()
        ProbeStore.refresh(this) { value ->
            if (!isDestroyed && !isFinishing) { state = value; render() }
        }
    }

    private fun select(blocking: Boolean) {
        denyButton.isEnabled = false
        emptyButton.isEnabled = false
        status.text = "Changing private fixture mode…"
        ProbeStore.selectBlocking(this, blocking) { value ->
            if (!isDestroyed && !isFinishing) { state = value; render() }
        }
    }

    private fun render() {
        val role = probeRole(this)
        val current = state
        val ready = role == ProbeRole.HELD && current?.ready == true
        status.text = buildString {
            appendLine("Screening role: ${role.name}")
            if (current == null) appendLine("Core/fixture: loading; not ready")
            else {
                appendLine("Fixture: ${current.alias}")
                appendLine("Selected mode: ${if (current.blocking) "exact deny" else "empty control"}")
                appendLine("Rust core: ${current.core}")
                current.error?.let { appendLine(it) }
                appendLine(if (ready) "Ready for manual observation — not proof of silence." else "Not ready for a valid blocking trial.")
            }
            if (role == ProbeRole.UNAVAILABLE) appendLine("This probe requires an available Android 10+ call-screening role.")
        }
        roleButton.isEnabled = role == ProbeRole.NOT_GRANTED || role == ProbeRole.ERROR
        denyButton.isEnabled = ready && current?.hasDeny == true
        emptyButton.isEnabled = current != null
        diagnostics.text = ProbeDiagnostics.latest
    }
}
