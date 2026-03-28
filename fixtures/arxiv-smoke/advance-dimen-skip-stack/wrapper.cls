\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\def\layoutmode{onecolumn}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\newdimen\wrapperdimen
\wrapperdimen=\p@
\DeclareOption{twocolumn}{\PassOptionsToClass{\CurrentOption}{article}\def\layoutmode{twocolumn}\advance\wrapperdimen by \p@}
\DeclareOption*{\PassOptionsToClass{\CurrentOption}{article}}
\ProcessOptions\relax
\edef\wrapperadvance{\the\wrapperdimen}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
