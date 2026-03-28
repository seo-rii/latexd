\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\newcount\wrapperlimit
\wrapperlimit=1
\newcount\wrappercount
\wrappercount=0
\def\wrappertrace{}
\DeclareOption{twocolumn}{\wrapperlimit=2\PassOptionsToClass{\CurrentOption}{article}}
\DeclareOption*{\PassOptionsToPackage{\CurrentOption}{shim}}
\ProcessOptions\relax
\loop
\advance\wrappercount by 1
\edef\wrappertrace{\wrappertrace W\number\wrappercount}
\ifnum\wrappercount<\wrapperlimit
\repeat
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
