# Third-party data

zil embeds the following data in its binaries and wasm build.

## FIGlet fonts

`crates/zil/src/modules/art/fonts/*.flf` (banner, big, bubble, digital, lean, mini, script, shadow, slant, small, smscript,
smshadow, smslant, standard) come unmodified from the [FIGlet 2.2.5](http://www.figlet.org/) distribution, under the
New BSD License:

```
Copyright (C) 1991, 1993, 1994 Glenn Chappell and Ian Chai
Copyright (C) 1996, 1997, 1998, 1999, 2000, 2001 John Cowan
Copyright (C) 2002 Christiaan Keet
Copyright (C) 2011, 2012 Claudio Matsuoka
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the distribution.
3. Neither the name of the author nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
SUCH DAMAGE.
```

## GeoNames

`crates/zil/src/modules/geo/cities.tsv` and `crates/zil/src/modules/geo/countries.tsv` are extracts of [GeoNames](https://www.geonames.org/)
data, licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Changes: cities are filtered to
capitals and places over 500,000 people, and both files keep only a subset of the columns.

## Time zones

The wasm build bundles the [IANA time zone database](https://www.iana.org/time-zones) (public domain) through
`jiff-tzdb`.

## JetBrains Mono

The Android app bundles `android/app/src/main/res/font/jetbrains_mono.ttf`, JetBrains Mono Regular 2.304 from
[JetBrains/JetBrainsMono](https://github.com/JetBrains/JetBrainsMono), unmodified, under the
[SIL Open Font License 1.1](https://openfontlicense.org/open-font-license-official-text/). Copyright 2020 The JetBrains
Mono Project Authors.
