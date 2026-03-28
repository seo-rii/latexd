\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\def\layoutmode{onecolumn}
\PassOptionsToPackage{unicode}{shim}
\DeclareOption{twocolumn}{\def\layoutmode{twocolumn}}
\makeatletter
\count@=\z@
\advance\count@ by \@ne
\@tempcnta=\tw@
\advance\@tempcnta\m@ne
\ProcessOptions\relax
\ifnum\count@=\@ne\def\wrappercount{wrapper-one}\else\def\wrappercount{wrapper-bad}\fi
\ifnum\@tempcnta=\@ne\def\wrapperscratch{scratch-one}\else\def\wrapperscratch{scratch-bad}\fi
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
