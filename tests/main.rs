use yew_template::*;
use yew::prelude::*;

struct Person {
    first_name: String,
    last_name: String,
}

fn main() {
    let boobool = false.to_string();
    let person = Person { first_name: "Edouard".to_string(), last_name: "G".to_string() };
    let _html = template_html!("tests/test.html", value="tes", value2={5.to_string()}, boobool, opt_value={Some("tes")}, opt_value2={Some("optvalue2")}, names_iter={["Edouart", "Foobar"].iter()}, background_color="#aaa", person);
}
