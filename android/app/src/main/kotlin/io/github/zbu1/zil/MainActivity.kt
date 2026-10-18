package io.github.zbu1.zil

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicTextField
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
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.TextFieldValue
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

private val mono = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 14.sp, color = Foreground)

/** The log, then a prompt: Enter runs the input unless it's unclosed, ↑/↓ walk the history, Stop interrupts. */
@Composable
fun Repl(exit: () -> Unit) {
    var input by remember { mutableStateOf(TextFieldValue()) }
    // How far back ↑ has gone; 0 is a fresh line.
    var back by remember { mutableIntStateOf(0) }
    val scroll = rememberLazyListState()
    LaunchedEffect(Zil.log.size) { if (Zil.log.isNotEmpty()) scroll.scrollToItem(Zil.log.lastIndex) }

    fun recall(to: Int) {
        back = to.coerceIn(0, Zil.history.size)
        val s = if (back == 0) "" else Zil.history[Zil.history.size - back]
        input = TextFieldValue(s, TextRange(s.length))
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
        LazyColumn(Modifier.weight(1f).fillMaxWidth().padding(horizontal = 8.dp), state = scroll) {
            items(Zil.log) { Text(it, Modifier.padding(vertical = 3.dp), style = mono) }
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
                modifier = Modifier.weight(1f).padding(vertical = 12.dp),
                textStyle = mono,
                cursorBrush = SolidColor(Foreground),
            )
            TextButton(onClick = { recall(back + 1) }) { Text("↑", style = mono) }
            TextButton(onClick = { recall(back - 1) }) { Text("↓", style = mono) }
            if (Zil.running) TextButton(onClick = Zil::stop) { Text("Stop", style = mono) }
        }
    }
}
