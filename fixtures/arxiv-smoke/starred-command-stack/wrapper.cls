\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\newcommand*{\wrapperflag}{wrapper-one}
\DeclareOption{twocolumn}{\renewcommand*{\wrapperflag}{wrapper-two}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
