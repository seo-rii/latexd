\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\newcount\wrappercount
\wrappercount=1
\DeclareOption{twocolumn}{\advance\wrappercount by 1}
\ProcessOptions\relax
\ifodd\wrappercount\def\wrapperguard{wrapper-odd}\else\def\wrapperguard{wrapper-even}\fi
\edef\wrappertail{wrapper-tail-\number\wrappercount}
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
