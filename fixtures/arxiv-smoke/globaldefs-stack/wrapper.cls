\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\def\wrapperflag{wrapper-plain}
\DeclareOption{twocolumn}{{\globaldefs=1\def\wrapperflag{wrapper-two}}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
