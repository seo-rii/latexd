\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\def\wrapperflag{\string\wrapperplain}
\DeclareOption{twocolumn}{\def\wrapperflag{\string\wrappertwo}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
