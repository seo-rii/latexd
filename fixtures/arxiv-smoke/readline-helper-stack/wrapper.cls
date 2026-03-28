\ProvidesClass{wrapper}[2024/01/01]
\def\layoutmode{onecolumn}
\DeclareOption{twocolumn}{\def\layoutmode{twocolumn}}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\newread\classstream
\openin\classstream={wrapper.cfg}
\readline\classstream to\classline
\closein\classstream
\ProcessOptions\relax
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
