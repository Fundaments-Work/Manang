//! Which package defines a command or an environment.
//!
//! `package_for` and `package_for_environment` are the tables `manang-latex`
//! has always used for its diagnostic hints, moved here unchanged so the hint
//! text cannot drift from the fixes. `command_packages` and
//! `environment_packages` extend them with more entries and with alternatives
//! (a command two packages provide): a fix is offered for each, never a guess.

/// The package that defines a commonly used command, for the "undefined
/// control sequence" hint. Only unambiguous, widely used ones.
pub fn package_for(command: &str) -> Option<&'static str> {
    Some(match command {
        "\\includegraphics" | "\\graphicspath" | "\\rotatebox" | "\\scalebox" | "\\resizebox" => {
            "graphicx"
        }
        "\\textcolor" | "\\color" | "\\definecolor" | "\\colorbox" | "\\fcolorbox" => "xcolor",
        "\\url" | "\\href" | "\\hyperref" | "\\autoref" | "\\nameref" => "hyperref",
        "\\mathbb" | "\\mathfrak" | "\\checkmark" | "\\leqslant" | "\\geqslant" => "amssymb",
        "\\text" | "\\tag" | "\\eqref" | "\\dfrac" | "\\tfrac" | "\\binom" | "\\boldsymbol"
        | "\\substack" | "\\intertext" | "\\operatorname" => "amsmath",
        "\\toprule" | "\\midrule" | "\\bottomrule" | "\\cmidrule" => "booktabs",
        "\\multirow" => "multirow",
        "\\SI" | "\\si" | "\\num" | "\\qty" | "\\unit" => "siunitx",
        "\\lstinline" | "\\lstlisting" | "\\lstset" => "listings",
        "\\subfloat" | "\\subcaption" | "\\captionof" => "subcaption",
        "\\cref" | "\\Cref" | "\\crefrange" => "cleveref",
        "\\citep" | "\\citet" | "\\citeauthor" | "\\citeyear" | "\\citealp" => "natbib",
        "\\bm" => "bm",
        "\\tikz" | "\\draw" | "\\node" | "\\fill" | "\\path" => "tikz",
        "\\includepdf" => "pdfpages",
        "\\FloatBarrier" => "placeins",
        "\\xspace" => "xspace",
        "\\cancel" | "\\bcancel" => "cancel",
        "\\hl" => "soul",
        "\\mathscr" => "mathrsfs",
        "\\mathbbm" => "bbm",
        "\\csvreader" => "csvsimple",
        "\\textbackslash" | "\\textless" => return None,
        _ => return None,
    })
}

pub fn package_for_environment(name: &str) -> Option<&'static str> {
    Some(
        match name.trim_matches(|c| c == '`' || c == '\'' || c == ' ') {
            "align" | "align*" | "equation*" | "gather" | "gather*" | "multline" | "multline*"
            | "split" | "cases" | "aligned" | "alignat" | "flalign" | "matrix" | "pmatrix"
            | "bmatrix" | "vmatrix" => "amsmath",
            "tikzpicture" => "tikz",
            "lstlisting" => "listings",
            "subfigure" | "subtable" => "subcaption",
            "algorithm" | "algorithmic" => "algorithm2e",
            "tabularx" => "tabularx",
            "longtable" => "longtable",
            "multicols" => "multicol",
            "theorem" | "lemma" | "proof" => "amsthm",
            _ => return None,
        },
    )
}

/// Commands from the extended table: `(commands, packages)`. The first
/// package is the usual one.
const EXTRA_COMMANDS: &[(&[&str], &[&str])] = &[
    (&["\\url"], &["hyperref", "url"]),
    (&["\\usetikzlibrary"], &["tikz"]),
    (
        &["\\addplot", "\\pgfplotsset", "\\addlegendentry"],
        &["pgfplots"],
    ),
    (
        &[
            "\\printbibliography",
            "\\addbibresource",
            "\\autocite",
            "\\parencite",
            "\\textcite",
            "\\footcite",
        ],
        &["biblatex"],
    ),
    (&["\\sout", "\\uline"], &["ulem"]),
    (
        &[
            "\\overset",
            "\\underset",
            "\\xrightarrow",
            "\\xleftarrow",
            "\\DeclareMathOperator",
        ],
        &["amsmath"],
    ),
    (&["\\mathds"], &["dsfont"]),
    (&["\\ifthenelse"], &["ifthen"]),
    (&["\\lipsum"], &["lipsum"]),
    (&["\\blindtext", "\\Blindtext"], &["blindtext"]),
    (&["\\todo"], &["todonotes"]),
    (&["\\enquote"], &["csquotes"]),
    (&["\\hypersetup"], &["hyperref"]),
    (&["\\tcbox", "\\newtcolorbox", "\\tcbset"], &["tcolorbox"]),
    (&["\\mintinline"], &["minted"]),
    (&["\\ding"], &["pifont"]),
    (&["\\nicefrac"], &["nicefrac"]),
    (&["\\captionsetup"], &["caption"]),
    (&["\\geometry", "\\newgeometry"], &["geometry"]),
    (&["\\fancyhead", "\\fancyfoot", "\\fancyhf"], &["fancyhdr"]),
    (
        &[
            "\\setstretch",
            "\\doublespacing",
            "\\singlespacing",
            "\\onehalfspacing",
        ],
        &["setspace"],
    ),
    (
        &[
            "\\DeclarePairedDelimiter",
            "\\coloneqq",
            "\\mathclap",
            "\\prescript",
        ],
        &["mathtools"],
    ),
    (
        &["\\SetKwInOut", "\\SetKw", "\\KwData", "\\KwResult"],
        &["algorithm2e"],
    ),
    (
        &["\\State", "\\Require", "\\Ensure", "\\Procedure"],
        &["algpseudocode"],
    ),
    (
        &["\\cellcolor", "\\rowcolor", "\\columncolor"],
        &["colortbl"],
    ),
    (&["\\makecell", "\\thead"], &["makecell"]),
];

fn lookup(
    table: &'static [(&'static [&'static str], &'static [&'static str])],
    name: &str,
) -> Option<&'static [&'static str]> {
    table
        .iter()
        .find(|(names, _)| names.contains(&name))
        .map(|(_, packages)| *packages)
        .filter(|packages| !packages.is_empty())
}

/// Every package that defines `command`: the usual one first. Empty when the
/// command is not in the tables (no fix is offered for it).
pub fn command_packages(command: &str) -> Vec<&'static str> {
    if let Some(found) = lookup(EXTRA_COMMANDS, command) {
        return found.to_vec();
    }
    package_for(command).into_iter().collect()
}

const EXTRA_ENVIRONMENTS: &[(&[&str], &[&str])] = &[
    (&["algorithm"], &["algorithm2e", "algorithm"]),
    (&["algorithmic"], &["algpseudocode", "algorithmic"]),
    (
        &["axis", "semilogxaxis", "semilogyaxis", "loglogaxis"],
        &["pgfplots"],
    ),
    (&["minted"], &["minted"]),
    (&["tcolorbox"], &["tcolorbox"]),
    (&["framed", "shaded", "leftbar"], &["framed"]),
    (&["wrapfigure", "wraptable"], &["wrapfig"]),
    (&["sidewaystable", "sidewaysfigure"], &["rotating"]),
    (&["landscape"], &["pdflscape", "lscape"]),
    (&["appendices"], &["appendix"]),
    (&["comment"], &["comment"]),
    (&["multicols*"], &["multicol"]),
    (
        &["singlespace", "doublespace", "onehalfspace", "spacing"],
        &["setspace"],
    ),
    (&["Verbatim", "BVerbatim", "LVerbatim"], &["fancyvrb"]),
    (
        &["dcases", "dcases*", "rcases", "multlined"],
        &["mathtools"],
    ),
    (&["subfigure", "subtable"], &["subcaption", "subfig"]),
    (&["longtable*"], &["longtable"]),
    (&["supertabular"], &["supertabular"]),
    (&["tabu"], &["tabu"]),
    (&["otherlanguage"], &["babel"]),
    (&["otherlanguage*"], &["babel"]),
    (&["CJK", "CJK*"], &["CJK"]),
    (
        &["cases*", "matrix*", "pmatrix*", "bmatrix*"],
        &["mathtools"],
    ),
];

/// Every package that defines the environment `name`, usual one first.
pub fn environment_packages(name: &str) -> Vec<&'static str> {
    let name = name.trim_matches(|c| c == '`' || c == '\'' || c == ' ');
    if let Some(found) = lookup(EXTRA_ENVIRONMENTS, name) {
        return found.to_vec();
    }
    package_for_environment(name).into_iter().collect()
}

/// Environments that `\newtheorem` defines (amsthm adds styles and `proof`,
/// but the environment itself needs a declaration).
pub fn is_theorem_like(name: &str) -> bool {
    matches!(
        name,
        "theorem" | "lemma" | "corollary" | "proposition" | "definition" | "remark" | "example"
    )
}

/// Packages a document class loads itself.
pub fn class_loads(class: &str) -> &'static [&'static str] {
    match class {
        "amsart" | "amsbook" | "amsproc" => &["amsmath", "amsthm", "amsfonts", "amssymb"],
        "revtex4" | "revtex4-1" | "revtex4-2" => &["amsmath", "amssymb", "graphicx", "xcolor"],
        "beamer" => &[
            "xcolor", "graphicx", "amsmath", "amssymb", "amsthm", "hyperref",
        ],
        "IEEEtran" => &["amsmath"],
        "llncs" => &["amsmath"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_tables_keep_the_legacy_first_choice() {
        for command in [
            "\\includegraphics",
            "\\textcolor",
            "\\url",
            "\\mathbb",
            "\\text",
            "\\toprule",
            "\\citep",
            "\\tikz",
        ] {
            assert_eq!(
                command_packages(command).first().copied(),
                package_for(command),
                "{command}"
            );
        }
        // The two environments whose hint was wrong are the only differences.
        assert_eq!(package_for_environment("algorithmic"), Some("algorithm2e"));
        assert_eq!(environment_packages("algorithmic")[0], "algpseudocode");
        assert_eq!(environment_packages("align*"), vec!["amsmath"]);
        assert!(environment_packages("nonesuch").is_empty());
    }

    #[test]
    fn alternatives_are_listed() {
        assert_eq!(command_packages("\\url"), vec!["hyperref", "url"]);
        assert_eq!(command_packages("\\printbibliography"), vec!["biblatex"]);
        assert!(command_packages("\\nonesuch").is_empty());
    }
}
