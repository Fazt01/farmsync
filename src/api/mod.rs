use crate::room::PlayerData;

pub fn player_list(players: &Vec<PlayerData>) -> String {
    let mut result = String::new();
    result.push_str(r#"<table id="room-list">"#);

    for p in players {
        result.push_str(player(p).as_str());
    }

    result.push_str(r#"</table>"#);
    result
}

pub fn player(player: &PlayerData) -> String {
    let button_text = if player.visited {
        "Already visited"
    } else {
        "Visit"
    };
    let disabled = if player.visited { " disabled" } else { "" };
    let button = format!(
        r#"
            <button hx-post="/visited"{disabled} hx-vals='{{"visited_id":{}}}'>
                {button_text}
            </button>
        "#,
        player.id,
    );

    format!(
        r#"
            <tr id="player-{}">
                <td>{}</td>
                <td>{}</td>
            </tr>"#,
        player.id, player.name, button,
    )
}
