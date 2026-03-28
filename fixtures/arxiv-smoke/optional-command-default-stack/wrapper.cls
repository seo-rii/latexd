\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\newcommand*{\wrapperflag}[1][wrapper-one]{#1}
\DeclareOption{twocolumn}{\renewcommand*{\wrapperflag}[1][wrapper-two]{#1}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
