use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use unicode_width::UnicodeWidthStr;

const TEXT: &str = r#"或日あるひの暮方の事である。一人の下人が、羅生門らしやうもんの下で雨やみを待つてゐた。
廣い門の下には、この男の外ほかに誰もゐない。唯、所々丹塗にぬりの剥げた、大きな圓柱まるばしらに、蟋蟀きり／″＼すが一匹とまつてゐる。羅生門らしやうもんが、朱雀大路すじやくおおぢにある以上いじやうは、この男の外にも、雨あめやみをする市女笠いちめがさや揉烏帽子が、もう二三人にんはありさうなものである。それが、この男をとこの外ほかには誰たれもゐない。
何故なぜかと云ふと、この二三年、京都には、地震ぢしんとか辻風とか火事とか饑饉とか云ふ災わざはひがつゞいて起つた。そこで洛中らくちうのさびれ方かたは一通りでない。舊記によると、佛像や佛具を打砕うちくだいて、その丹にがついたり、金銀の箔はくがついたりした木を、路ばたにつみ重ねて、薪たきぎの料しろに賣つてゐたと云ふ事である。洛中らくちうがその始末であるから、羅生門の修理しゆりなどは、元より誰も捨てゝ顧かへりみる者がなかつた。するとその荒あれ果はてたのをよい事にして、狐狸こりが棲む。盗人ぬすびとが棲む。とうとうしまひには、引取ひきとり手のない死人を、この門へ持つて來て、棄てゝ行くと云ふ習慣しふくわんさへ出來た。そこで、日の目が見えなくなると、誰でも氣味きみを惡るがつて、この門の近所きんじよへは足あしぶみをしない事になつてしまつたのである。
その代り又鴉からすが何處どこからか、たくさん集つて來た。晝間ひるま見みると、その鴉が何羽なんばとなく輪を描いて高い鴟尾しびのまはりを啼なきながら、飛びまはつてゐる。殊に門の上の空が、夕燒ゆふやけであかくなる時ときには、それが胡麻ごまをまいたやうにはつきり見えた。鴉からすは、勿論、門の上にある死人しにんの肉を、啄みに來るのである。――尤も今日は、刻限こくげんが遲おそいせいか、一羽も見えない。唯、所々ところどころ、崩れかゝつた、さうしてその崩くづれ目に長い草のはへた石段いしだんの上に、鴉からすの糞くそが、點々と白くこびりついてゐるのが見える。下人げにんは七段ある石段の一番上の段だんに洗あらひざらした紺こんの襖あをの尻を据ゑて、右の頬に出來た、大きな面皰にきびを氣にしながら、ぼんやり、雨あめのふるのを眺ながめてゐるのである。"#;

fn chars_fold(s: &str, width: usize) -> String {
    if width == 0 {
        return String::from("");
    }

    s.chars().fold(String::from(""), |acc: String, c: char| {
        let last_line = acc.lines().last().unwrap_or(&acc);
        if last_line.width() + c.to_string().width() > width {
            format!("{acc}\n{c}")
        } else {
            format!("{acc}{c}")
        }
    })
}

fn for_chars(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let mut result = String::with_capacity(s.len() + s.len() / width);
    let mut current_line_width = 0;

    for c in s.chars() {
        let char_width = UnicodeWidthStr::width(c.encode_utf8(&mut [0; 4]));

        if current_line_width + char_width > width {
            result.push('\n');
            current_line_width = char_width;
        } else {
            current_line_width += char_width;
        }

        result.push(c);
    }

    result
}

fn benchmark(c: &mut Criterion) {
    c.bench_function("chars-fold", |b| {
        b.iter(|| chars_fold(black_box(TEXT), black_box(20)))
    });

    c.bench_function("for-chars", |b| {
        b.iter(|| for_chars(black_box(TEXT), black_box(20)))
    });
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
