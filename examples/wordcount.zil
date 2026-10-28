# Run: zil examples/wordcount.zil notes.txt
#  or: cat notes.txt | zil examples/wordcount.zil
# Counts lines and words, then charts the most common words.

config.permissions = ["files.read", "stdin"]
text = if args().len > 0 { read_file(args()[0]) } else { input }
words = text.lower.words

print("lines: {text.lines.len}, words: {words.len}, unique: {words.unique.len}")
print(words.count_by(|w| w).entries.sort(|[_, n]| -n).take(10).from_entries.bars)
