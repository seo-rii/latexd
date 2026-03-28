\ProvidesClass{wrapper}[2024/01/01]
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\newdimen\wrapperdimen
\newskip\wrapperskip
\wrapperdimen=1.5pt
\wrapperskip=2pt plus 1pt
\DeclareOption{twocolumn}{\PassOptionsToClass{\CurrentOption}{article}\wrapperdimen=2.5pt}
\DeclareOption*{\PassOptionsToClass{\CurrentOption}{article}}
\ProcessOptions\relax
\edef\wrapperdimcheck{\ifdim\wrapperdimen<\wrapperskip LT\else GE\fi/\ifdim\wrapperskip>\p@ GT\else LE\fi}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
