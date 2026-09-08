package com.antoniopantano.callerfilter

import android.os.Bundle
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import uniffi.callerfilter_core.coreVersion
import uniffi.callerfilter_core.defaultEntryLimit

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Proves Kotlin is talking to the same Rust core the iOS app uses.
        val text = TextView(this).apply {
            text = "Caller Filter\ncore ${coreVersion()}\niOS entry limit ${defaultEntryLimit()}"
            textSize = 18f
            setPadding(48, 96, 48, 48)
        }
        setContentView(text)
    }
}
