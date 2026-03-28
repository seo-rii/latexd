\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{wrapper}[2024/01/01 Wrapper class fixture]
\PassOptionsToPackage{unicode}{shim}
\def\writeprefix{CLASS}
\newwrite\classstream
\immediate\openout\classstream={\jobname.clsaux}
\write\classstream{\writeprefix-WRITE-\jobname}
\closeout\classstream
\LoadClassWithOptions{article}
