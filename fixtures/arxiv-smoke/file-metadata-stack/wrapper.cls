\ProvidesClass{wrapper}[2024/02/01 v2.3 Wrapper class fixture]
\def\wrapperclass{wrapper}
\edef\wrappermeta{[\filedate][\fileversion][\fileinfo]}
\DeclareOption{twocolumn}{\PassOptionsToClass{\CurrentOption}{article}}
\ProcessOptions\relax
\LoadClassWithOptions{article}
