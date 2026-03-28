\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\def\wrappermode{wrapper-one}
\PassOptionsToPackage{unicode}{shim}
\DeclareOption{twocolumn}{\def\wrappermode{wrapper-two}}
\ProcessOptions\relax
\makeatletter
\@xa\def\csname wrapperseed\endcsname{\wrappermode}
\@xa\edef\csname wrapperdeferred\endcsname{\@nx\wrapperseed}
{\def\wrapperseed{local-\wrappermode}\xdef\wrapperxdeferred{\@nx\wrapperseed}}
\def\wrapperseed{mutated-\wrappermode}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
