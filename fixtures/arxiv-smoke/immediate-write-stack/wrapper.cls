\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{wrapper}[2024/01/01 Wrapper class fixture]
\PassOptionsToPackage{unicode}{shim}
\def\writeprefix{CLASS}
\makeatletter
\immediate\write\@auxout{\writeprefix-WRITE-\jobname}
\makeatother
\LoadClassWithOptions{article}
