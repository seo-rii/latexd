\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\def\wrapperflag{wrapperplain}
\DeclareOption{twocolumn}{\def\wrapperflag{wrappertwo}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
