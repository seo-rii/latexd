\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\makeatletter
\def\wrappercar{wrapper-car-\@car abc\@nil}
\def\wrappercdr{wrapper-cdr-\@cdr abc\@nil}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
