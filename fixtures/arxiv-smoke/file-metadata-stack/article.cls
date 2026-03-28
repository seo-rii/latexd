\ProvidesClass{article}[2024/01/01 Local article fixture]
\def\articleclass{article}
\def\layoutmode{onecolumn}
\DeclareOption{twocolumn}{\def\layoutmode{twocolumn}}
\ProcessOptions\relax
\edef\articlemeta{[\filedate][\fileversion][\fileinfo]}
