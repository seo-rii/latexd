\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\def\wrapperflag{wrapper-one}
\def\wrapperprobe{\futurelet\wrappernext\wrapperprobeaux}
\def\wrapperprobeaux{\ifx\wrappernext[OPT\else PLAIN\fi}
\DeclareOption{twocolumn}{\def\wrapperflag{wrapper-two}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
