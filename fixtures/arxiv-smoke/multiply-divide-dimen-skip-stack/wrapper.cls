\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\def\wrappermode{onecolumn}
\makeatletter
\newdimen\wrapperdimen
\wrapperdimen=2pt
\DeclareOption{twocolumn}{\def\wrappermode{twocolumn}\multiply\wrapperdimen by \thr@@\divide\wrapperdimen by \tw@}
\DeclareOption*{\PassOptionsToClass{\CurrentOption}{article}}
\ProcessOptions\relax
\edef\wrapperarithmetic{\the\wrapperdimen}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
