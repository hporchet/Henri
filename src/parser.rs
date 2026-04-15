use crate::{
    tokeniser::{preprocessing, tokenization, CssToken},
    utils::{self, CharStream, StreamIterator, TokenStream},
};
use log::{self, debug, error, trace, warn};
use url::Url;

#[allow(dead_code)]
#[derive(Debug)]
pub struct Declaration {
    name: String,
    component_values: Vec<ComponentValue>,
    important: bool,
    original_text: Option<String>,
}

#[derive(Debug)]
pub enum ComponentValue {
    PreservedToken(CssToken),
    Function {
        name: String,
        component_value: Vec<ComponentValue>,
    },
    SimpleBlock {
        associated_token: CssToken,
        value: Vec<ComponentValue>,
    },
}

#[derive(Debug)]
pub enum Rule {
    AtRule {
        name: String,
        component_value: Vec<ComponentValue>,
    },
    BlockAtRule {
        name: String,
        component_value: Vec<ComponentValue>,
        declarations: Vec<Declaration>,
        child_rules: Vec<Rule>,
    },
    QualifiedRule {
        component_value: Vec<ComponentValue>,
        declarations: Vec<Declaration>,
        child_rules: Vec<Rule>,
    },
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct CssStyleSheet {
    type_sheet: String,
    location: String,
    parent: Option<Box<CssStyleSheet>>,
    // media:
    title: String,
    alternate: bool,
    disabled: bool,
    rules: Vec<Rule>,
    origin_clean: bool,
    constructed: bool,
    disallow_modification: bool,
    // constructor_document
    base_url: String,
}

impl CssStyleSheet {
    fn new(location: Url, rules: Vec<Rule>) -> CssStyleSheet {
        CssStyleSheet {
            type_sheet: "StyleSheet".to_string(),
            location: location.to_string(),
            parent: None,
            title: location.path().to_string(),
            alternate: false,
            disabled: false,
            rules,
            origin_clean: false,
            constructed: false,
            disallow_modification: false,
            base_url: location
                .domain()
                .unwrap_or("Host not get in the url")
                .to_string(),
        }
    }
}

impl From<Vec<Rule>> for CssStyleSheet {
    fn from(rules: Vec<Rule>) -> CssStyleSheet {
        CssStyleSheet {
            type_sheet: "StyleSheet".to_string(),
            location: "".to_string(),
            parent: None,
            title: "".to_string(),
            alternate: false,
            disabled: false,
            rules,
            origin_clean: false,
            constructed: false,
            disallow_modification: false,
            base_url: "".to_string(),
        }
    }
}

#[derive(Debug)]
pub enum ParseError {
    GetFileError(utils::ReadFileError),
    UnknowToken(CssToken),
    ParseError(String),
    NoToken,
    BadToken(String),
}

impl From<utils::ReadFileError> for ParseError {
    fn from(value: utils::ReadFileError) -> Self {
        ParseError::GetFileError(value)
    }
}

/// <https://drafts.csswg.org/css-syntax/#consume-stylesheet-contents>
pub fn parse_stylesheet_url(url: Url) -> Result<CssStyleSheet, ParseError> {
    let datastream = utils::get_data(&url)?;
    debug!("File get");

    let res = parse_string(datastream);

    match res {
        Ok(rules) => Ok(CssStyleSheet::new(url, rules)),
        Err(err) => Err(err),
    }
}

pub fn parse_stylesheet(datastream: String) -> Result<CssStyleSheet, ParseError> {
    match parse_string(datastream) {
        Ok(rules) => Ok(CssStyleSheet::from(rules)),
        Err(err) => Err(err),
    }
}

/// The real parser brain, parse a StyleSheet stocked in a string
fn parse_string(datastream: String) -> Result<Vec<Rule>, ParseError> {
    let mut token_stream = normalize(datastream);
    debug!("Stream tokenize");
    let mut rules: Vec<Rule> = Vec::new();
    while let Some(token) = token_stream.peek() {
        match token {
            CssToken::WhitespaceToken | CssToken::CdcToken | CssToken::CdoToken => {
                token_stream.next();
            }
            CssToken::AtKeywordToken(_) => {
                if let Some(rule) = consume_at_rule(&mut token_stream, false) {
                    rules.push(rule);
                } else {
                    debug!("at_ruled failed");
                    token_stream.next();
                }
            }
            _ => {
                if let Some(rule) = consume_qualified_rule(&mut token_stream, None, false) {
                    rules.push(rule);
                } else {
                    debug!("qualified_rule failed");
                    token_stream.next();
                }
            }
        }
    }

    Ok(rules)
}

/// <https://drafts.csswg.org/css-syntax/#consume-at-rule>
///
/// Assert: The next token is an `<at-keyword-token>`.
///
/// Consume a token from input, and let rule be a new at-rule with its name set to the returned token’s value, its prelude initially set to an empty list, and no declarations or child rules.
///
/// Process input:
///
/// * \<semicolon-token> \<EOF-token>
///     * Discard a token from input. If rule is valid in the current context, return it; otherwise return nothing.
/// * <}-token>
///     * If nested is true:
///         * If rule is valid in the current context, return it.
///         * Otherwise, return nothing.
///     * Otherwise, consume a token and append the result to rule’s prelude.
/// * <{-token>
///     * Consume a block from input, and assign the results to rule’s lists of declarations and child rules.
///     If rule is valid in the current context, return it. Otherwise, return nothing.
/// * anything else
///     * Consume a component value from input and append the returned value to rule’s prelude.
fn consume_at_rule(tokens: &mut impl StreamIterator<CssToken>, nested: bool) -> Option<Rule> {
    if let Some(CssToken::AtKeywordToken(name)) = tokens.peek() {
        tokens.next();
        let mut prelude: Vec<ComponentValue> = Vec::new();
        let mut child_rules: Vec<Rule> = Vec::new();
        let mut declarations: Vec<Declaration> = Vec::new();

        while let Some(token) = tokens.peek() {
            match token {
                CssToken::SemicolonToken => {
                    debug!("ameno");
                    tokens.next();
                    if !child_rules.is_empty() || !declarations.is_empty() {
                        return Some(Rule::BlockAtRule {
                            name,
                            component_value: prelude,
                            declarations,
                            child_rules,
                        });
                    }
                    return Some(Rule::AtRule {
                        name,
                        component_value: prelude,
                    });
                }
                CssToken::AcoladeClToken => {
                    if nested {
                        if !child_rules.is_empty() || !declarations.is_empty() {
                            return Some(Rule::BlockAtRule {
                                name,
                                component_value: prelude,
                                declarations,
                                child_rules,
                            });
                        }
                        return None;
                    }
                    prelude.push(ComponentValue::PreservedToken(token));
                }
                CssToken::AcoladeOpToken => {
                    let (mut xdeclarations, mut xchilds) = consume_block(tokens);
                    declarations.append(&mut xdeclarations);
                    child_rules.append(&mut xchilds);
                    if !child_rules.is_empty() || !declarations.is_empty() {
                        return Some(Rule::BlockAtRule {
                            name,
                            component_value: prelude,
                            declarations,
                            child_rules,
                        });
                    }
                }
                _ => {
                    let val = consume_component_value(tokens);
                    if val.is_ok() {
                        prelude.push(val.ok()?);
                    } else {
                        error!("Error when parsing a component value {:#?}", val.err()?);
                    }
                }
            }
        }

        return Some(Rule::AtRule {
            name,
            component_value: prelude,
        });
    }
    None
}

/// <https://drafts.csswg.org/css-syntax/#consume-a-block>
///
/// To consume a block, from a token stream input:
/// Assert: The next token is a `<{-token>`.
///
/// Let decls be an empty list of declarations, and rules be an empty list of rules.
///
/// Discard a token from input. Consume a block’s contents from input and assign the results to decls and rules.
/// Discard a token from input.
///
/// Return decls and rules.
fn consume_block(tokens: &mut impl StreamIterator<CssToken>) -> (Vec<Declaration>, Vec<Rule>) {
    debug!("consume_block");
    if let Some(CssToken::AcoladeOpToken) = tokens.peek() {
        tokens.next();
        return consume_block_content(tokens);
    }
    (Vec::new(), Vec::new())
}

/// <https://drafts.csswg.org/css-syntax/#consume-block-contents>
///
/// Let decls be an empty list of declarations, and rules be an empty list of rules.
///
/// Process input:
///
/// * \<whitespace-token> \<semicolon-token>
///     * Discard a token from input.
/// * \<EOF-token> <}-token>
///     * Return decls and rules.
/// * \<at-keyword-token>
///     * Consume an at-rule from input, with nested set to true. If a rule was returned, append it to rules.
/// * anything else
///     * Mark input.
///     * Consume a declaration from input, with nested set to true. If a declaration was returned, append it to decls, and discard a mark from input.
///     * Otherwise, restore a mark from input, then consume a qualified rule from input, with nested set to true, and `<semicolon-token>` as the stop token. If a rule was returned, append it to rules.
fn consume_block_content(
    tokens: &mut impl StreamIterator<CssToken>,
) -> (Vec<Declaration>, Vec<Rule>) {
    debug!("consume_block_content");
    let mut rules: Vec<Rule> = Vec::new();
    let mut declarations: Vec<Declaration> = Vec::new();

    while let Some(token) = tokens.peek() {
        match token {
            CssToken::WhitespaceToken | CssToken::SemicolonToken => tokens.next(),
            CssToken::AcoladeClToken => break,
            CssToken::AtKeywordToken(_) => {
                let at_rule = consume_at_rule(tokens, true);
                if at_rule.is_some() {
                    rules.push(at_rule.unwrap());
                }
            }
            _ => {
                debug!("-> mark for introspection");
                tokens.mark();
                if let Some(decls) = consume_declaration(tokens, true) {
                    debug!("declaration found");
                    declarations.push(decls);
                    tokens.discard_mark();
                } else {
                    debug!("testing qualified rule");
                    tokens.unmark();
                    if let Some(rls) =
                        consume_qualified_rule(tokens, Some(CssToken::SemicolonToken), true)
                    {
                        debug!("qualified found");
                        rules.push(rls);
                    }
                }
            }
        }
    }

    debug!("consume_block_content end");
    (declarations, rules)
}

/// <https://drafts.csswg.org/css-syntax/#consume-a-declaration>
/// Let decl be a new declaration, with an initially empty name and a value set to an empty list.
/// 1. If the next token is an `<ident-token>`, consume a token from input and set decl’s name to the token’s value.
///
///     Otherwise, consume the remnants of a bad declaration from input, with nested, and return nothing.
/// 2. Discard whitespace from input.
/// 3. If the next token is a `<colon-token>`, discard a token from input.
///
///     Otherwise, consume the remnants of a bad declaration from input, with nested, and return nothing.
/// 4. Discard whitespace from input.
/// 5. Consume a list of component values from input, with nested, and with `<semicolon-token>` as the stop token, and set decl’s value to the result.
/// 6. If the last two non-`<whitespace-token>`s in decl’s value are a `<delim-token>` with the value "!" followed by an `<ident-token>` with a value that is an ASCII case-insensitive match for "important", remove them from decl’s value and set decl’s important flag.
/// 7. While the last item in decl’s value is a <whitespace-token>, remove that token.
/// 8. If decl’s name is a custom property name string, then set decl’s original text to the segment of the original source text string corresponding to the tokens of decl’s value.
///
///     Otherwise, if decl’s value contains a top-level simple block with an associated token of `<{-token>`, and also contains any other non-<whitespace-token> value, return nothing. (That is, a top-level {}-block is only allowed as the entire value of a non-custom property.)
///
///     Otherwise, if decl’s name is an ASCII case-insensitive match for "unicode-range", consume the value of a unicode-range descriptor from the segment of the original source text string corresponding to the tokens returned by the consume a list of component values call, and replace decl’s value with the result.
/// 9. If decl is valid in the current context, return it; otherwise return nothing.
fn consume_declaration(
    tokens: &mut impl StreamIterator<CssToken>,
    nested: bool,
) -> Option<Declaration> {
    debug!("consume_declaration");
    let name;
    let mut component_values: Vec<ComponentValue> = Vec::new();
    let mut important = false;
    let original_text = None;

    if let Some(CssToken::IdentToken(name_tok)) = tokens.peek() {
        name = name_tok;
        tokens.next();
    } else {
        consume_bad_declaration(tokens, nested);
        return None;
    }

    discard_whitespace(tokens);

    if let Some(CssToken::ColonToken) = tokens.peek() {
        tokens.next();
    } else {
        consume_bad_declaration(tokens, nested);
        return None;
    }

    discard_whitespace(tokens);

    component_values.append(&mut consume_component_list_value(
        tokens,
        Some(CssToken::SemicolonToken),
        nested,
    ));

    if (component_values.len() >= 2) {
        if let Some(ComponentValue::PreservedToken(CssToken::DelimToken('!'))) =
            component_values.get(component_values.len() - 2)
        {
            if let Some(ComponentValue::PreservedToken(CssToken::IdentToken(txt))) =
                component_values.get(component_values.len() - 1)
            {
                if txt == &String::from("important") {
                    important = true;
                    component_values.pop();
                    component_values.pop();
                }
            }
        }
    }

    while matches!(
        component_values.last(),
        Some(&ComponentValue::PreservedToken(CssToken::WhitespaceToken))
    ) {
        let _ = component_values.pop();
    }

    Some(Declaration {
        name,
        component_values,
        important,
        original_text,
    })
}

/// <https://drafts.csswg.org/css-syntax/#consume-a-list-of-component-values>
/// To consume a list of component values from a token stream input, given an optional token stop token and an optional boolean nested (default false):
///
/// Let values be an empty list of component values.
///
/// Process input:
/// * \<eof-token>, stop token (if passed)
///
///     Return values.
/// * <}-token>
///
///     If nested is true, return values.
///
///     Otherwise, this is a parse error. Consume a token from input and append the result to values.
/// * anything else
///
///     Consume a component value from input, and append the result to values.
fn consume_component_list_value(
    tokens: &mut impl StreamIterator<CssToken>,
    stop_token: Option<CssToken>,
    nested: bool,
) -> Vec<ComponentValue> {
    let mut res: Vec<ComponentValue> = Vec::new();

    while let Some(token) = tokens.peek() {
        match token {
            CssToken::AcoladeClToken => {
                if nested {
                    return res;
                }
                // parse error
                tokens.next();
            }
            _ => {
                if stop_token.is_some() && &token == stop_token.as_ref().unwrap() {
                    return res;
                }
                let comp_val = consume_component_value(tokens);
                if comp_val.is_ok() {
                    res.push(comp_val.unwrap());
                }
            }
        }
    }

    res
}

fn discard_whitespace(tokens: &mut impl StreamIterator<CssToken>) {
    while let Some(CssToken::WhitespaceToken) = tokens.peek() {
        tokens.next();
    }
}

/// To consume the remnants of a bad declaration from a token stream input, given a bool nested:
///
/// Process input:
///
/// * \<eof-token>, \<semicolon-token>
///     Discard a token from input, and return nothing.
/// * <}-token>
///     If nested is true, return nothing. Otherwise, discard a token.
/// * anything else
///     Consume a component value from input, and do nothing.
fn consume_bad_declaration(tokens: &mut impl StreamIterator<CssToken>, nested: bool) {
    while let Some(token) = tokens.peek() {
        match token {
            CssToken::SemicolonToken => {
                tokens.next();
                return;
            }
            CssToken::AcoladeClToken => {
                if nested {
                    return;
                }
                tokens.next();
            }
            _ => {
                tokens.next();
                let _ = consume_component_value(tokens);
            }
        }
    }
    tokens.next();
}

/// <https://drafts.csswg.org/css-syntax/#consume-qualified-rule>
///
/// Let rule be a new qualified rule with its prelude, declarations, and child rules all initially set to empty lists.
/// Process input:
/// * \<EOF-token> stop token (if passed)
///     * This is a parse error. Return nothing.
/// * <}-token>
///     * This is a parse error. If nested is true, return nothing. Otherwise, consume a token and append the result to rule’s prelude.
/// * <{-token>
///     * If the first two non-<whitespace-token> values of rule’s prelude are an \<ident-token> whose value starts with "--" followed by a `<colon-token>`, then:
///         * If nested is true, consume the remnants of a bad declaration from input, with nested set to true, and return nothing.
///         * If nested is false, consume a block from input, and return nothing.
///         * Otherwise, consume a block from input, and assign the results to rule’s lists of declarations and child rules.
/// If rule is valid in the current context, return it; otherwise return nothing.
/// * anything else
///     * Consume a component value from input and append the result to rule’s prelude.
fn consume_qualified_rule(
    tokens: &mut impl StreamIterator<CssToken>,
    stop_token: Option<CssToken>,
    nested: bool,
) -> Option<Rule> {
    let mut prelude: Vec<ComponentValue> = Vec::new();

    while let Some(token) = tokens.peek() {
        if let Some(ref stop) = stop_token {
            if token == *stop {
                debug!("Stop token found");
                return None;
            }
        }
        match token {
            CssToken::AcoladeClToken => {
                if nested {
                    warn!("parser error in consuming qualified rule Close Token encounter");
                    return None;
                }
                prelude.push(ComponentValue::PreservedToken(token));
                tokens.next();
            }
            CssToken::AcoladeOpToken => {
                let (declarations, rules) = consume_block(tokens);
                return Some(Rule::QualifiedRule {
                    component_value: prelude,
                    declarations,
                    child_rules: rules,
                });
            }
            _ => {
                let val = consume_component_value(tokens);
                if val.is_ok() {
                    prelude.push(val.ok()?);
                } else {
                    error!("Error when parsing a component value {:#?}", val.err()?);
                    return None;
                }
            }
        }
    }
    None
}

/// <https://drafts.csswg.org/css-syntax/#consume-a-component-value>
/// To consume a component value from a token stream input:
/// Process input:
/// * <{-token> <[-token> `<(-token>`
///     * Consume a simple block from input and return the result.
/// * \<function-token>
///     * Consume a function from input and return the result.
/// * anything else
///     * Consume a token from input and return the result.
fn consume_component_value(
    tokens: &mut impl StreamIterator<CssToken>,
) -> Result<ComponentValue, ParseError> {
    if let Some(token) = tokens.peek() {
        match token {
            CssToken::AcoladeOpToken | CssToken::CrochetOpToken | CssToken::ParenthOpToken => {
                return consume_simple_bloc(tokens);
            }
            CssToken::FunctionToken(_) => return consume_function(tokens),
            _ => {
                tokens.next();
                return Ok(ComponentValue::PreservedToken(token));
            }
        }
    }

    Err(ParseError::NoToken)
}

/// <https://drafts.csswg.org/css-syntax/#consume-a-function>
/// To consume a function from a token stream input:
/// Assert: The next token is a `<function-token>`.
///
/// Consume a token from input, and let function be a new function with its name equal the returned token’s value, and a value set to an empty list.
/// Process input:
/// * \<eof-token> <)-token>
///     * Discard a token from input. Return function.
/// * anything else
///     * Consume a component value from input and append the result to function’s value.
fn consume_function(
    tokens: &mut impl StreamIterator<CssToken>,
) -> Result<ComponentValue, ParseError> {
    if let Some(CssToken::FunctionToken(name)) = tokens.peek() {
        let mut values: Vec<ComponentValue> = Vec::new();

        tokens.next();
        while let Some(token) = tokens.peek() {
            match token {
                CssToken::ParenthClToken => {
                    tokens.next();
                    break;
                }
                _ => {
                    let val = consume_component_value(tokens);
                    if val.is_ok() {
                        values.push(val.unwrap());
                    } else {
                        error!("error in function body parsing {:#?}", val.err())
                    }
                }
            }
        }
        return Ok(ComponentValue::Function {
            name,
            component_value: values,
        });
    }
    Err(ParseError::ParseError(String::from(
        "consuming function without function token ??",
    )))
}

/// <https://drafts.csswg.org/css-syntax/#consume-a-simple-block>
/// To consume a simple block from a token stream input:
///
/// Assert: the next token of input is `<{-token>`, `<[-token>`, or `<(-token>`.
///
/// Let ending token be the mirror variant of the next token. (E.g. if it was called with `<[-token>`, the ending token is `<]-token>`.)
///
/// Let block be a new simple block with its associated token set to the next token and with its value initially set to an empty list.
///
/// Discard a token from input.
///
/// Process input:
/// * \<eof-token> ending token
///     * Discard a token from input. Return block.
/// * anything else
///     * Consume a component value from input and append the result to block’s value.
fn consume_simple_bloc(
    tokens: &mut impl StreamIterator<CssToken>,
) -> Result<ComponentValue, ParseError> {
    if let Some(ending) = tokens.peek() {
        if matches!(
            ending,
            CssToken::AcoladeOpToken | CssToken::CrochetOpToken | CssToken::ParenthOpToken
        ) {
            tokens.next();
            let mut comp_values: Vec<ComponentValue> = Vec::new();

            while let Some(token) = tokens.peek() {
                if token == ending {
                    break;
                }
                let res = consume_component_value(tokens);
                if res.is_ok() {
                    comp_values.push(res.unwrap());
                }
            }

            return Ok(ComponentValue::SimpleBlock {
                associated_token: ending,
                value: comp_values,
            });
        }
        return Err(ParseError::BadToken(String::from("No valid opening token")));
    }
    Err(ParseError::NoToken)
}

/// <https://drafts.csswg.org/css-syntax/#normalize-into-a-token-stream>
/// To normalize into a token stream a given input:
///
/// If input is already a token stream, return it.
///
/// If input is a list of CSS tokens and/or component values, create a new token stream with input as its tokens, and return it.
///
/// If input is a string, then filter code points from input, tokenize the result, then create a new token stream with those tokens as its tokens, and return it.
///
/// Assert: Only the preceding types should be passed as input.
fn normalize(input: String) -> TokenStream {
    //impl StreamIterator<CssToken> {
    let mut input_stream = CharStream::new(preprocessing(input));
    let mut tokens: Vec<CssToken> = Vec::new();

    while input_stream.peek().is_some() {
        match tokenization(&mut input_stream) {
            Err(err) => {
                log::error!("{}", err);
                break;
            }
            Ok(token) => {
                tokens.push(token);
            }
        }

        if tokens.len() > 1 && tokens.get(tokens.len() - 1) == tokens.get(tokens.len() - 2) {
            warn!("token already found {:#?}", tokens.get(tokens.len() - 1));
            break;
        }
        trace!("last token found {:#?}", tokens.get(tokens.len() - 1));
    }

    TokenStream::new(tokens)
}

#[cfg(test)]
mod parser_test {
    use std::{env, fs::File, io::Read};

    use log::{error, info};
    use url::Url;

    use crate::parser::{
        consume_declaration, parse_stylesheet, parse_stylesheet_url, ComponentValue,
    };
    use crate::utils::test_utils::init_test_logger;

    use super::{consume_at_rule, consume_function, consume_simple_bloc, normalize};

    #[test]
    fn normalize_test() {
        init_test_logger();

        let mut file = File::open("./test/style.css").unwrap();
        let mut buffer = String::new();
        let _ = file.read_to_string(&mut buffer).unwrap();
        normalize(buffer);

        assert!(true);
    }

    #[test]
    fn simple_block_test() {
        init_test_logger();

        let simple_block = "{
            color: white;
        }";
        let mut tokens = normalize(simple_block.to_string());

        let res = consume_simple_bloc(&mut tokens);

        match res {
            Err(err) => {
                info!("error for parsing this block {:#?}", err);
                assert!(false);
            }
            Ok(_) => {
                assert!(true);
            }
        }
    }

    #[test]
    fn function_test() {
        init_test_logger();

        let simple_function = "circle(50px)";
        let mut tokens = normalize(simple_function.to_string());
        info!("{:#?}", tokens);

        let res = consume_function(&mut tokens);

        match res {
            Err(err) => {
                info!("error for parsing this function {:#?}", err);
                assert!(false)
            }
            Ok(res) => {
                info!("{:#?}", res);
                assert!(true)
            }
        }
    }

    #[test]
    fn at_rule_test() {
        init_test_logger();

        let at_input = "@charset utf8;";
        let mut tokens = normalize(at_input.to_string());

        let res = consume_at_rule(&mut tokens, false);
        match res {
            None => {
                error!("error no rule can be parsed");

                assert!(false);
            }
            Some(_rule) => {
                assert!(true)
            }
        }
    }

    #[test]
    fn balise_color_test() {
        init_test_logger();
        let css = "a {
            color: white;
        }";

        let _res = parse_stylesheet(css.to_string());
    }

    #[test]
    fn global_url_parse_test() {
        init_test_logger();

        let work_dir = env::current_dir().unwrap();
        let location =
            Url::from_file_path(format!("{}/test/style.css", work_dir.display())).unwrap();
        let res = parse_stylesheet_url(location);
        log::debug!("{:#?}", res);
        match res {
            Err(err) => {
                log::error!("{:#?}", err);
                assert!(false)
            }
            Ok(parsed_css) => {
                assert_eq!("StyleSheet", parsed_css.type_sheet);
                assert_ne!(0, parsed_css.rules.len())
            }
        }
    }

    #[test]
    fn test_simple_declaration() {
        init_test_logger();
        let css = "color: red;";
        let mut tokens = normalize(css.to_string());
        let decl = consume_declaration(&mut tokens, false);
        assert!(decl.is_some());
        let decl = decl.unwrap();
        assert_eq!(decl.name, "color");
        assert_eq!(decl.component_values.len(), 1);
        assert!(!decl.important);
    }

    #[test]
    fn test_declaration_with_important() {
        init_test_logger();
        let css = "margin: 10px !important;";
        let mut tokens = normalize(css.to_string());
        let decl = consume_declaration(&mut tokens, false);
        assert!(decl.is_some());
        let decl = decl.unwrap();
        assert_eq!(decl.name, "margin");
        assert!(decl.important);
        assert_eq!(decl.component_values.len(), 1); // "10px" seulement, "!important" est supprimé
    }

    #[test]
    fn test_invalid_declaration_missing_colon() {
        init_test_logger();
        let css = "color red;";
        let mut tokens = normalize(css.to_string());
        let decl = consume_declaration(&mut tokens, false);
        assert!(decl.is_none()); // Doit retourner None
    }

    #[test]
    fn test_declaration_with_function() {
        init_test_logger();
        let css = "transform: rotate(45deg);";
        let mut tokens = normalize(css.to_string());
        let decl = consume_declaration(&mut tokens, false);
        assert!(decl.is_some());
        let decl = decl.unwrap();
        assert_eq!(decl.name, "transform");
        log::debug!("{:#?}", &decl);
        assert_eq!(decl.component_values.len(), 1);
        if let ComponentValue::Function {
            name,
            component_value,
        } = &decl.component_values[0]
        {
            assert_eq!(name, "rotate");
            assert_eq!(component_value.len(), 1);
        } else {
            panic!("Expected a function component value");
        }
    }
}
