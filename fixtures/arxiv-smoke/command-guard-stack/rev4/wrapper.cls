\ProvidesClass{wrapper}[2024/01/04]
\newcommand{\guarded}{class-guard}
\newcommand{\guarded}{class-dup}
\DeclareOption{twocolumn}{\PassOptionsToClass{\CurrentOption}{article}}
\DeclareOption*{\PassOptionsToPackage{\CurrentOption}{shim}}
\ProcessOptions\relax
\LoadClass{article}
\RequirePackage{shim}
