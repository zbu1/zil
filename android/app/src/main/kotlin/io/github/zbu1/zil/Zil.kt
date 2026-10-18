package io.github.zbu1.zil

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.graphics.Color
import java.io.File

/**
 * The REPL session (crates/zil-android, on its own native thread) and what the screen shows of it. One per process,
 * so the log and history outlive the activity.
 */
object Zil {
    init {
        System.loadLibrary("zil_android")
    }

    private external fun start(files: String, cache: String)
    private external fun submit(src: String)
    external fun stop()
    /** Whether Enter should continue `src` on a new line instead of running it. */
    external fun unclosed(src: String): Boolean
    /** `src` with numbers, strings and keywords colored, as ANSI. */
    external fun highlight(src: String): String
    private external fun complete(before: String, word: String): String

    /** Completions for the name ending at the cursor, or after a `.`; none while something runs. */
    fun completions(before: String, word: String): List<String> =
        if (running || (word.isEmpty() && !before.endsWith('.'))) emptyList() else complete(before, word).split('\n').filter { it.isNotEmpty() }

    val log = mutableStateListOf<AnnotatedString>()
    var running by mutableStateOf(false)
        private set
    /** Past inputs, oldest first, kept across launches. */
    val history = mutableListOf<String>()

    private lateinit var historyFile: File
    private lateinit var clipboard: ClipboardManager
    private val main = Handler(Looper.getMainLooper())
    /** Printed lines not yet on screen: a busy loop's prints land as one log entry per frame, not one post per line. */
    private val pending = StringBuilder()

    fun init(context: Context) {
        if (::historyFile.isInitialized) return
        clipboard = context.getSystemService(ClipboardManager::class.java)
        historyFile = File(context.filesDir, "history")
        if (historyFile.exists()) history += historyFile.readText().split('\u0000').filter { it.isNotEmpty() }
        start(context.filesDir.path, File(context.cacheDir, "zil").path)
    }

    fun run(src: String) {
        if (history.lastOrNull() != src) history += src
        if (history.size > 1000) history.removeAt(0)
        historyFile.writeText(history.joinToString("\u0000"))
        log += ansi(highlight(src).lines().mapIndexed { i, l -> (if (i == 0) "\u001b[2m> \u001b[0m" else "\u001b[2m. \u001b[0m") + l }.joinToString("\n"))
        running = true
        submit(src)
    }

    // Called from the session thread.

    fun print(text: String) {
        synchronized(pending) {
            if (pending.isEmpty()) main.post(::flush)
            pending.append(text).append('\n')
        }
    }

    fun done() {
        main.post {
            flush()
            running = false
        }
    }

    fun copy(text: String) = clipboard.setPrimaryClip(ClipData.newPlainText("zil", text))

    fun paste(): String = clipboard.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.text?.toString() ?: ""

    private fun flush() {
        val text = synchronized(pending) { pending.toString().also { pending.clear() } }
        if (text.isEmpty()) return
        log += ansi(text.removeSuffix("\n"))
        // ponytail: drops the oldest entries past 2000; a runaway print loop can still make one huge entry.
        if (log.size > 2000) log.removeRange(0, log.size - 2000)
    }
}

val Foreground = Color(0xFFDCDFE4)
val Background = Color(0xFF16181C)
private val dim = Color(0xFF8A8F98)
private val basic = listOf(0xFF5C6370, 0xFFE06C75, 0xFF98C379, 0xFFE5C07B, 0xFF61AFEF, 0xFFC678DD, 0xFF56B6C2, 0xFFDCDFE4).map { Color(it) }
private val csi = Regex("\u001b\\[([0-9;?]*)([A-Za-z])")

/** The xterm 256-color palette: the 16 basic colors, a 6×6×6 cube, then grays. */
private fun xterm(n: Int): Color {
    if (n < 16) return basic[n % 8]
    if (n >= 232) return (8 + 10 * (n - 232)).let { Color(it, it, it) }
    val level = { v: Int -> if (v == 0) 0 else 55 + 40 * v }
    return Color(level((n - 16) / 36), level((n - 16) / 6 % 6), level((n - 16) % 6))
}

/** Terminal output as styled text: SGR colors become spans, other escapes (cursor moves, clears) are dropped. */
fun ansi(s: String): AnnotatedString = buildAnnotatedString {
    var style = SpanStyle()
    var at = 0
    for (m in csi.findAll(s)) {
        withStyle(style) { append(s.substring(at, m.range.first)) }
        at = m.range.last + 1
        if (m.groupValues[2] != "m") continue
        val codes = m.groupValues[1].split(';').map { it.toIntOrNull() ?: 0 }.iterator()
        // `38;5;n` and `38;2;r;g;b` take the next few codes too.
        fun extended(): Color? = when (if (codes.hasNext()) codes.next() else 0) {
            5 -> if (codes.hasNext()) xterm(codes.next()) else null
            2 -> List(3) { if (codes.hasNext()) codes.next() else 0 }.let { (r, g, b) -> Color(r, g, b) }
            else -> null
        }
        while (codes.hasNext()) {
            style = when (val c = codes.next()) {
                0 -> SpanStyle()
                1 -> style.copy(fontWeight = FontWeight.Bold)
                2 -> style.copy(color = dim)
                7 -> style.copy(color = Background, background = Foreground)
                22 -> style.copy(fontWeight = null, color = Color.Unspecified)
                27 -> style.copy(color = Color.Unspecified, background = Color.Unspecified)
                in 30..37 -> style.copy(color = basic[c - 30])
                in 90..97 -> style.copy(color = basic[c - 90])
                38 -> style.copy(color = extended() ?: Color.Unspecified)
                39 -> style.copy(color = Color.Unspecified)
                in 40..47 -> style.copy(background = basic[c - 40])
                48 -> style.copy(background = extended() ?: Color.Unspecified)
                49 -> style.copy(background = Color.Unspecified)
                else -> style
            }
        }
    }
    withStyle(style) { append(s.substring(at)) }
}
