\ProvidesClass{wrapper}[2024/01/05]
\newcommand{\guarded}{class-guard}
\DeclareOption{twocolumn}{\PassOptionsToClass{\CurrentOption}{article}}
\DeclareOption*{\PassOptionsToPackage{\CurrentOption}{shim}}
\ProcessOptions\relax
\LoadClass{article}
\RequirePackage{shim}
