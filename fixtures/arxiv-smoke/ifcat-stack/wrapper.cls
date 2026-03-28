\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\def\wrapperseed{wrapper-one}
\PassOptionsToPackage{unicode}{shim}
\DeclareOption{twocolumn}{\def\wrapperseed{wrapper-two}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
