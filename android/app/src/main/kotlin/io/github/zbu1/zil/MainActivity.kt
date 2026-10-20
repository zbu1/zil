package io.github.zbu1.zil

import android.os.Bundle
import android.text.InputType
import android.view.HapticFeedbackConstants
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
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.InterceptPlatformTextInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.PlatformTextInputMethodRequest
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Popup
import kotlinx.coroutines.launch
import kotlin.math.floor

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
@OptIn(ExperimentalLayoutApi::class, ExperimentalComposeUiApi::class)
@Composable
fun Repl(exit: () -> Unit) {
    var input by remember { mutableStateOf(TextFieldValue()) }
    // How far back ↑ has gone; 0 is a fresh line.
    var back by remember { mutableIntStateOf(0) }
    var focused by remember { mutableStateOf(false) }
    // The line plus newline that Enter just ran, until the keyboard sends something else.
    var ran by remember { mutableStateOf<String?>(null) }
    // `clear session` or `clear all`, waiting on its dialog.
    var confirming by remember { mutableStateOf<String?>(null) }
    val scroll = rememberLazyListState()
    val prompt = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    val view = LocalView.current
    // Dismissing the keyboard leaves the prompt, taking the extra keys with it.
    val ime = WindowInsets.isImeVisible
    val focus = LocalFocusManager.current
    LaunchedEffect(ime) { if (!ime) focus.clearFocus() }
    // New output jumps to the end of the log; keyed on the last entry too, since past the cap the size stops changing.
    LaunchedEffect(Zil.log.size, Zil.log.lastOrNull()) { scroll.toEnd() }

    fun recall(to: Int) {
        back = to.coerceIn(0, Zil.history.size)
        val s = if (back == 0) "" else Zil.history[Zil.history.size - back]
        input = TextFieldValue(s, TextRange(s.length))
    }

    fun press(key: String) {
        // An extra key is a fresh edit, so rerunning the line ↑ recalls isn't mistaken for the keyboard's stale copy.
        ran = null
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
                val s = if (key.all { it.isLetter() }) (if (text.take(at).endsWith(' ')) "$key " else " $key ") else key
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
            "clear history" -> Zil.clearHistory()
            "clear results" -> Zil.clear("clear results")
            "clear session", "clear all" -> confirming = src.trim()
            else -> Zil.run(src)
        }
    }

    confirming?.let { c ->
        AlertDialog(
            onDismissRequest = { confirming = null },
            title = { Text("$c?") },
            text = { Text("This forgets every variable, function and unit.") },
            confirmButton = {
                TextButton({
                    confirming = null
                    if (c == "clear all") {
                        Zil.clearHistory()
                        Zil.log.clear()
                    }
                    Zil.clear("clear session")
                }) { Text("Clear") }
            },
            dismissButton = { TextButton({ confirming = null }) { Text("Cancel") } },
            containerColor = Background,
            titleContentColor = Foreground,
            textContentColor = Foreground,
        )
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
            // Scrolled up, a button jumps back to the end.
            if (scroll.canScrollForward) {
                val scope = rememberCoroutineScope()
                TextButton(
                    onClick = { scope.launch { scroll.toEnd() } },
                    Modifier.align(Alignment.BottomEnd).padding(8.dp).background(Pressed),
                ) { Text("↓", style = mono) }
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
                    // `to` brings its space, so the units it converts to show next.
                    val add = if (hit == "to") "to " else hit
                    val s = before.dropLast(word.length) + add + input.text.substring(at)
                    input = TextFieldValue(s, TextRange(at - word.length + add.length))
                }) { Text(hit, style = mono) }
            }
        }
        Row(Modifier.padding(start = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("> ", style = mono)
            // Suggestions off, so Enter can't commit one: keyboards ignore the bare no-suggestions flag but honor it on a visible password.
            InterceptPlatformTextInput({ request, next ->
                next.startInputMethod(PlatformTextInputMethodRequest { info ->
                    request.createInputConnection(info).also {
                        info.inputType = info.inputType and InputType.TYPE_MASK_VARIATION.inv() or
                            InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
                    }
                })
            }) {
                BasicTextField(
                    value = input,
                    onValueChange = { new ->
                        // A lone newline typed into a complete input runs it, as Enter does in the terminal.
                        val at = new.selection.start
                        val typedNewline = new.text.length == input.text.length + 1 && at > 0 && new.text[at - 1] == '\n' &&
                            new.text.removeRange(at - 1, at) == input.text
                        when {
                            // The keyboard can send one more edit on its copy of the line it just ran before it sees the line cleared.
                            new.text == ran -> {}
                            !typedNewline -> input = new
                            Zil.running -> {}
                            !Zil.unclosed(input.text) -> {
                                ran = new.text
                                enter(input.text)
                            }
                            else -> input = new
                        }
                        if (new.text != ran) ran = null
                    },
                    modifier = Modifier.weight(1f).focusRequester(prompt).onFocusChanged { focused = it.isFocused }.padding(vertical = 12.dp),
                    textStyle = mono,
                    visualTransformation = colored,
                    cursorBrush = SolidColor(Foreground),
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
                )
            }
            if (Zil.running) TextButton(onClick = Zil::stop) { Text("Stop", style = mono) }
        }
        if (focused && ime) for (row in extraKeys) Row(Modifier.fillMaxWidth()) {
            for ((i, keys) in row.withIndex()) {
                val (key, alts) = keys.split(' ').let { it[0] to it.drop(1) }
                var down by remember { mutableStateOf(false) }
                // The alternate under the finger while the long-press popup is open.
                var picked by remember { mutableStateOf<Int?>(null) }
                var size by remember { mutableStateOf(IntSize.Zero) }
                // The popup starts over the key, shifted left by however many keys it would overhang the row's end.
                val shift = (i + alts.size - row.size).coerceAtLeast(0)
                // Arrows fire on touch and repeat while held; the rest fire on release, or open their alternates on a long press.
                val gesture = Modifier.pointerInput(key) {
                    awaitEachGesture {
                        awaitFirstDown()
                        down = true
                        view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                        try {
                            if (key in "←→↑↓") {
                                press(key)
                                if (withTimeoutOrNull(400) { waitForUpOrCancellation(); true } == null) {
                                    do press(key) while (withTimeoutOrNull(50) { waitForUpOrCancellation(); true } == null)
                                }
                            } else {
                                when (withTimeoutOrNull(viewConfiguration.longPressTimeoutMillis) { waitForUpOrCancellation() != null }) {
                                    true -> press(key)
                                    false -> {}
                                    null if alts.isEmpty() -> press(key)
                                    null -> {
                                        view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                                        picked = 0
                                        while (true) {
                                            val change = awaitPointerEvent().changes.first()
                                            change.consume()
                                            if (!change.pressed) break
                                            picked = (floor(change.position.x / size.width).toInt() + shift).coerceIn(alts.indices)
                                        }
                                        press(alts[picked!!])
                                    }
                                }
                            }
                        } finally {
                            down = false
                            picked = null
                        }
                    }
                }
                Box(Modifier.weight(1f).onSizeChanged { size = it }.background(if (down) Pressed else Background).then(gesture), contentAlignment = Alignment.Center) {
                    // The font has ⇥ but not ⇤, so ⇤ is ⇥ mirrored rather than a fallback glyph.
                    val label = Modifier.padding(vertical = 10.dp)
                    if (key == "⇤") Text("⇥", label.graphicsLayer(scaleX = -1f), style = mono) else Text(key, label, style = mono)
                    picked?.let { p ->
                        Popup(offset = IntOffset(-shift * size.width, -size.height)) {
                            Row(Modifier.background(Pressed)) {
                                for ((j, alt) in alts.withIndex()) {
                                    val cell = with(LocalDensity.current) { Modifier.size(size.width.toDp(), size.height.toDp()) }
                                    Box(cell.background(if (j == p) Picked else Pressed), contentAlignment = Alignment.Center) { Text(alt, style = mono) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/** Scrolls to the bottom edge of the last entry, not its top, so a tall output shows its end. */
private suspend fun LazyListState.toEnd() {
    if (layoutInfo.totalItemsCount == 0) return
    scrollToItem(layoutInfo.totalItemsCount - 1)
    while (scrollBy(10_000f) > 0f) {}
}

/**
 * Keys the soft keyboard buries, as in Termux, each followed by the alternates a long press offers, the first preselected:
 * arrows and Home/End (⇤ ⇥) on the right, zil's common symbols on the left.
 */
private val extraKeys = listOf(
    listOf("⇤", "( { , :", ") } ;", "\" r\" $ € £", "/ ** * ^ // % mod ~", "= == != <= >= += -= !", "↑", "⇥"),
    listOf("to in of for if else fn", "[ < <= <<", "] > >= >>", "|> .. ..= | || & &&", "_ . # @ \\", "←", "↓", "→"),
)

private val Pressed = Color(0xFF2C3038)
private val Picked = Color(0xFF4A5060)
