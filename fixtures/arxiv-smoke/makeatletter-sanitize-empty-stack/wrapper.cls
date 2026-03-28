\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\PassOptionsToPackage{unicode}{shim}
\def\wrappermode{onecolumn}
\DeclareOption{twocolumn}{\def\wrappermode{twocolumn}}
\ProcessOptions\relax
\makeatletter
\def\wrapperraw{W}
\@onlypreamble\wrapperraw
\@onelevel@sanitize\wrapperraw
\@bsphack
\@esphack
\edef\wrapperzap{\zap@space w r a p p e r \@empty}
\def\wrapperempty{\@ifempty{}{E}{N}}
\def\wrappernonempty{\@ifnotempty{ x }{N}{E}}
\makeatother
\LoadClassWithOptions{article}
\RequirePackageWithOptions{shim}
