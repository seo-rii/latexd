\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\newcount\wrappercount
\wrappercount=1
\DeclareOption{twocolumn}{\advance\wrappercount by 1}
\ProcessOptions\relax
\unless\ifodd\wrappercount\def\wrapperguard{wrapper-even}\else\def\wrapperguard{wrapper-odd}\fi
\edef\wrappertail{wrapper-tail-\number\wrappercount}
\makeatletter
\unless\ifcsname wrapperhook\endcsname\def\wrappermissing{wrapper-missing}\else\def\wrappermissing{wrapper-present}\fi
\@namedef{wrapperhook}{wrapper-hook}
\unless\ifcsname wrapperhook\endcsname\def\wrapperdefined{wrapper-undefined}\else\def\wrapperdefined{wrapper-defined}\fi
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
