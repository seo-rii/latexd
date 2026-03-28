\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\def\layoutmode{onecolumn}
\DeclareOption{twocolumn}{\def\layoutmode{twocolumn}}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\count@=\tw@
\multiply\count@ by \thr@@
\divide\count@ by \tw@
\@tempcnta=8
\divide\@tempcnta by \tw@
\ProcessOptions\relax
\ifnum\count@=\thr@@\def\wrapperproduct{wrapper-product}\else\def\wrapperproduct{wrapper-bad}\fi
\ifnum\@tempcnta=4\def\wrapperquotient{wrapper-quotient}\else\def\wrapperquotient{wrapper-bad}\fi
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
