\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\def\layoutmode{onecolumn}
\DeclareOption{twocolumn}{\def\layoutmode{twocolumn}}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\newcount\wrappercountreg
\wrappercountreg=\@ne
\advance\wrappercountreg by \tw@
\ProcessOptions\relax
\ifnum\wrappercountreg=3\def\wrappercount{wrapper-count}\else\def\wrappercount{wrapper-bad}\fi
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
