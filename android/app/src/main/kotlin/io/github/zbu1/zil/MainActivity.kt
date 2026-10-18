package io.github.zbu1.zil

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        Zil.init(this)
        setContent { Repl(exit = ::finish) }
    }
}

/** A bundled font: the system monospace lacks box drawing, and its fallback glyphs are wider, skewing boxes. No ligatures. */
private val mono = TextStyle(fontFamily = FontFamily(Font(R.font.jetbrains_mono)), fontSize = 14.sp, color = Foreground, fontFeatureSettings = "calt 0")

/** Input colored as it's typed; the text itself is untouched, so offsets map one to one. */
private val colored = VisualTransformation { TransformedText(ansi(Zil.highlight(it.text)), OffsetMapping.Identity) }

/**
 * The log, then completions for the word at the cursor, then a prompt: Enter runs the input unless it's unclosed,
 * Stop interrupts, and while it's focused a bar of extra keys sits below it: ↑/↓ walk the history.
 */
@Composable
fun Repl(exit: () -> Unit) {
    var input by remember { mutableStateOf(TextFieldValue()) }
    // How far back ↑ has gone; 0 is a fresh line.
    var back by remember { mutableIntStateOf(0) }
    var focused by remember { mutableStateOf(false) }
    val scroll = rememberLazyListState()
    val prompt = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    LaunchedEffect(Zil.log.size) { if (Zil.log.isNotEmpty()) scroll.scrollToItem(Zil.log.lastIndex) }

    fun recall(to: Int) {
        back = to.coerceIn(0, Zil.history.size)
        val s = if (back == 0) "" else Zil.history[Zil.history.size - back]
        input = TextFieldValue(s, TextRange(s.length))
    }

    fun press(key: String) {
        val text = input.text
        val at = input.selection.min
        fun move(to: Int) {
            input = input.copy(selection = TextRange(to.coerceIn(0, text.length)))
        }
        when (key) {
            "↑" -> recall(back + 1)
            "↓" -> recall(back - 1)
            "←" -> move(at - 1)
            "→" -> move(input.selection.max + 1)
            "⇤" -> move(text.lastIndexOf('\n', at - 1) + 1)
            "⇥" -> move(text.indexOf('\n', at).takeIf { it >= 0 } ?: text.length)
            else -> {
                // Keywords come padded with spaces, minus one already before the cursor.
                val s = if (key[0].isLetter()) (if (text.take(at).endsWith(' ')) "$key " else " $key ") else key
                input = TextFieldValue(text.replaceRange(at, input.selection.max, s), TextRange(at + s.length))
            }
        }
    }

    fun enter(src: String) {
        input = TextFieldValue()
        back = 0
        when (src.trim()) {
            "" -> {}
            "exit", "quit" -> exit()
            "clear" -> Zil.log.clear()
            else -> Zil.run(src)
        }
    }

    Column(Modifier.fillMaxSize().background(Background).safeDrawingPadding()) {
        val measurer = rememberTextMeasurer()
        // Tapping the log types at the prompt, as in a terminal; long-pressing it selects instead.
        val tapped = Modifier.clickable(interactionSource = null, indication = null) {
            prompt.requestFocus()
            keyboard?.show()
        }
        BoxWithConstraints(Modifier.weight(1f).fillMaxWidth().then(tapped).padding(horizontal = 8.dp)) {
            // Measured over 100 characters, so rounding one glyph's width doesn't cost a column.
            val hundred = remember(measurer) { measurer.measure("0".repeat(100), mono).size.width }
            LaunchedEffect(constraints.maxWidth) { Zil.columns(constraints.maxWidth * 100 / hundred) }
            SelectionContainer {
                LazyColumn(Modifier.fillMaxSize(), state = scroll) {
                    items(Zil.log) { Text(it, Modifier.padding(vertical = 3.dp), style = mono) }
                }
            }
        }
        // The name before the cursor, as the terminal REPL's Tab sees it; tapping a completion replaces it.
        val at = input.selection.start
        val before = input.text.substring(0, at)
        val word = before.takeLastWhile { it.isLetterOrDigit() || it == '_' }
        val hits = remember(input.text, at, Zil.running) { Zil.completions(before.dropLast(word.length), word).take(50) }
        if (hits.isNotEmpty() && hits != listOf(word)) LazyRow(Modifier.fillMaxWidth().padding(horizontal = 4.dp)) {
            items(hits) { hit ->
                TextButton(onClick = {
                    val s = before.dropLast(word.length) + hit + input.text.substring(at)
                    input = TextFieldValue(s, TextRange(at - word.length + hit.length))
                }) { Text(hit, style = mono) }
            }
        }
        Row(Modifier.padding(start = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("> ", style = mono)
            BasicTextField(
                value = input,
                onValueChange = { new ->
                    // A lone newline typed into a complete input runs it, as Enter does in the terminal.
                    val at = new.selection.start
                    val typedNewline = new.text.length == input.text.length + 1 && at > 0 && new.text[at - 1] == '\n' &&
                        new.text.removeRange(at - 1, at) == input.text
                    when {
                        !typedNewline -> input = new
                        Zil.running -> {}
                        !Zil.unclosed(input.text) -> enter(input.text)
                        else -> input = new
                    }
                },
                modifier = Modifier.weight(1f).focusRequester(prompt).onFocusChanged { focused = it.isFocused }.padding(vertical = 12.dp),
                textStyle = mono,
                visualTransformation = colored,
                cursorBrush = SolidColor(Foreground),
            )
            if (Zil.running) TextButton(onClick = Zil::stop) { Text("Stop", style = mono) }
        }
        if (focused) for (row in extraKeys) Row(Modifier.fillMaxWidth()) {
            for ((key, alt) in row) {
                // Arrows fire on touch and repeat while held; the rest fire on release, or their alternate on a long press.
                val gesture = Modifier.pointerInput(key) {
                    awaitEachGesture {
                        awaitFirstDown()
                        if (alt == null && key in "←→↑↓") {
                            press(key)
                            if (withTimeoutOrNull(400) { waitForUpOrCancellation(); true } == null) {
                                do press(key) while (withTimeoutOrNull(50) { waitForUpOrCancellation(); true } == null)
                            }
                        } else {
                            when (withTimeoutOrNull(viewConfiguration.longPressTimeoutMillis) { waitForUpOrCancellation() != null }) {
                                true -> press(key)
                                null -> press(alt ?: key)
                                false -> {}
                            }
                        }
                    }
                }
                Box(Modifier.weight(1f).then(gesture).padding(vertical = 10.dp), contentAlignment = Alignment.Center) {
                    // The font has ⇥ but not ⇤, so ⇤ is ⇥ mirrored rather than a fallback glyph.
                    if (key == "⇤") Text("⇥", Modifier.graphicsLayer(scaleX = -1f), style = mono) else Text(key, style = mono)
                }
            }
        }
    }
}

/**
 * Keys the soft keyboard buries, as in Termux, each with its long-press alternate: arrows and Home/End (⇤ ⇥) on the right,
 * zil's common symbols on the left.
 */
private val extraKeys = listOf(
    listOf("⇤" to null, "(" to "{", ")" to "}", "\"" to "r\"", "^" to "**", "=" to "==", "↑" to null, "⇥" to null),
    listOf("to" to "in", "[" to "<", "]" to ">", "|>" to "..", "%" to "mod", "←" to null, "↓" to null, "→" to null),
)
