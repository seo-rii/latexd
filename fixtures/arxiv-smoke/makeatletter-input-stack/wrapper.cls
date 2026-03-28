\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\def\layoutmode{onecolumn}
\PassOptionsToPackage{unicode}{shim}
\DeclareOption{twocolumn}{\def\layoutmode{twocolumn}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
