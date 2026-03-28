\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\def\layoutmode{plain}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\newskip\wrapperskip
\wrapperskip=10pt plus 2pt
\skipdef\wrapperalias=252
\wrapperalias=1.5pt plus 1pt
\skip@=3pt minus 1pt
\@tempskipa=0.5pt plus 0.25pt
\DeclareOption{twocolumn}{\def\layoutmode{twocolumn}\wrapperskip=12pt plus 3pt}
\DeclareOption{onecolumn}{\def\layoutmode{onecolumn}\wrapperskip=9pt plus 1pt}
\ProcessOptions\relax
\edef\wrapperskips{\the\wrapperskip|\the\wrapperalias|\the\skip@|\the\@tempskipa|\the\p@}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
