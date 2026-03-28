\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\@xp\def\csname wrappermode\endcsname{wrapper-one}
\DeclareOption{twocolumn}{\@xp\def\csname wrappermode\endcsname{wrapper-two}}
\makeatother
\ProcessOptions\relax
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
