\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\def\wrapperhook{hook}
\DeclareOption{twocolumn}{}
\ProcessOptions\relax
\ifcsname wrapperhook\endcsname\def\wrapperguard{wrapper-present}\else\def\wrapperguard{wrapper-missing}\fi
\ifcsname wrappermissing\endcsname\def\wrappermissingguard{wrapper-unexpected}\else\def\wrappermissingguard{wrapper-absent}\fi
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
