\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{wrapper}[2024/01/01 Wrapper class fixture]
\DeclareOption{broken}{\makeatletter\@latexerr{class failure}{remove broken}\makeatother}
\PassOptionsToPackage{unicode}{shim}
\LoadClassWithOptions{article}
\ProcessOptions\relax
