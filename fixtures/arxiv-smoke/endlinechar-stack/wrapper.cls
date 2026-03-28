\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{wrapper}[2024/01/01 Wrapper class fixture]
\def\layoutmode{onecolumn}
\DeclareOption{twocolumn}{\def\layoutmode{twocolumn}\endlinechar=`\!}
\DeclareOption{onecolumn}{\def\layoutmode{onecolumn}\endlinechar=`\#}
\ProcessOptions\relax
\newread\classstream
\openin\classstream={wrapper.cfg}
\read\classstream to\classline
\closein\classstream
\PassOptionsToPackage{unicode}{shim}
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
