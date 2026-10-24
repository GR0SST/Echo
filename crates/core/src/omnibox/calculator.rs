//! Bounded, local arithmetic evaluation for the address field. The evaluator
//! uses neither a script engine nor a network service.

pub fn answer(input: &str) -> Option<String> {
    if input.len() > 512 { return None; }
    let input = input.trim().trim_end_matches('=').trim();
    // A number alone remains an ordinary search.
    if !input.chars().any(|c| matches!(c, '+' | '-' | '*' | '/' | '^' | '%')) { return None; }
    let mut parser = Parser { rest: input.as_bytes(), depth: 0 };
    let value = parser.expression()?;
    parser.whitespace();
    if !parser.rest.is_empty() || !value.is_finite() { return None; }
    let value = if value == 0. { 0. } else { value };
    Some(format!("{value}"))
}

struct Parser<'a> { rest: &'a [u8], depth: usize }
impl Parser<'_> {
    fn whitespace(&mut self) {
        while self.rest.first().is_some_and(u8::is_ascii_whitespace) { self.rest = &self.rest[1..]; }
    }
    fn take(&mut self, byte: u8) -> bool {
        self.whitespace();
        if self.rest.first() == Some(&byte) { self.rest = &self.rest[1..]; true } else { false }
    }
    fn expression(&mut self) -> Option<f64> {
        let mut value = self.product()?;
        loop {
            if self.take(b'+') { value += self.product()?; }
            else if self.take(b'-') { value -= self.product()?; }
            else { return Some(value); }
        }
    }
    fn product(&mut self) -> Option<f64> {
        let mut value = self.unary()?;
        loop {
            if self.take(b'*') { value *= self.unary()?; }
            else if self.take(b'/') { value /= self.unary()?; }
            else { return Some(value); }
        }
    }
    fn unary(&mut self) -> Option<f64> {
        self.depth += 1;
        if self.depth > 64 { return None; }
        let result = if self.take(b'+') { self.unary() }
            else if self.take(b'-') { self.unary().map(|value| -value) }
            else { self.power() };
        self.depth -= 1;
        result
    }
    fn power(&mut self) -> Option<f64> {
        let mut value = self.atom()?;
        while self.take(b'%') { value /= 100.; }
        if self.take(b'^') { value = value.powf(self.unary()?); }
        Some(value)
    }
    fn atom(&mut self) -> Option<f64> {
        if self.take(b'(') {
            let value = self.expression()?;
            return self.take(b')').then_some(value);
        }
        self.whitespace();
        let length = self.rest.iter().take_while(|c| c.is_ascii_digit() || **c == b'.').count();
        if length == 0 { return None; }
        let value = std::str::from_utf8(&self.rest[..length]).ok()?.parse().ok()?;
        self.rest = &self.rest[length..];
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::answer;
    #[test]
    fn arithmetic_obeys_precedence_and_parentheses() {
        for (input, expected) in [("2+2", "4"), ("2+3*4", "14"), ("(2+3)*4", "20"),
            ("-2^2", "-4"), ("2^3^2", "512"), ("200*15%", "30"), ("1.5/2=", "0.75")] {
            assert_eq!(answer(input).as_deref(), Some(expected), "{input}");
        }
    }
    #[test]
    fn incomplete_unsafe_or_nonfinite_expressions_are_not_answers() {
        for input in ["42", "2+", "http://2+2.com", "1/0", "0/0", "2^99999", "(2+2", "2+2 words", "2**3", "alert(1)"] {
            assert_eq!(answer(input), None, "{input}");
        }
        assert_eq!(answer(&format!("{}2+2{}", "(".repeat(100), ")".repeat(100))), None);
    }
}
