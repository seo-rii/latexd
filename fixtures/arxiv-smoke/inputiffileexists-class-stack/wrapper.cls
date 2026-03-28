\ProvidesClass{wrapper}[2024/01/01]
\def\wrapperclass{wrapper}
\def\cfgbody{}
\InputIfFileExists{wrapper.cfg}{\def\cfgstatus{cfg-loaded}}{\def\cfgstatus{cfg-missing}}
\LoadClassWithOptions{article}
