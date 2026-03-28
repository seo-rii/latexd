\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{wrapper}[2024/01/01 Wrapper class fixture]
\PassOptionsToPackage{unicode}{shim}
\newread\classstream
\openin\classstream={wrapper.cfg}
\read\classstream to\classline
\ifeof\classstream
  \def\classeof{eof}
\else
  \def\classeof{open}
\fi
\closein\classstream
\LoadClassWithOptions{article}
