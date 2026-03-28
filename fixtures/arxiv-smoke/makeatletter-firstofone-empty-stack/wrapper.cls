\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\let\wrapperempty\@empty
\let\wrappernil\@nil
\def\wrapperfirst{\@firstofone{wrapper-first}}
\def\wrapperguard{\ifx\wrapperempty\@empty E\else X\fi\ifx\wrappernil\@nil N\else X\fi\wrapperfirst}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
