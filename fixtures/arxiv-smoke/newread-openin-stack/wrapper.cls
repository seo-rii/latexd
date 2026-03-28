\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{wrapper}[2024/01/01 Wrapper class fixture]
\PassOptionsToPackage{unicode}{shim}
\newread\classstream
\openin\classstream={wrapper.cfg}
\ifeof\classstream
  \def\classread{missing}
\else
  \def\classread{open}
\fi
\closein\classstream
\ifeof\classstream
  \def\classclosed{closed}
\else
  \def\classclosed{open}
\fi
\LoadClassWithOptions{article}
