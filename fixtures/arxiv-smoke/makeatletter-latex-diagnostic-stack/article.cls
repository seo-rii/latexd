\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{article}[2024/01/01 Local article fixture]
\def\layoutmode{onecolumn}
\DeclareOption{onecolumn}{\def\layoutmode{onecolumn}}
\DeclareOption{twocolumn}{\def\layoutmode{twocolumn}}
\ProcessOptions\relax
