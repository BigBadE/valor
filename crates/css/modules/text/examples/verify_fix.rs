//! Verify the line-height fix works correctly

use css_orchestrator::style_model::ComputedStyle;
use css_text::measurement::measure_text;

fn main() {
    println!("=== Line-Height Fix Verification ===\n");
    
    let test_cases = vec![
        // (font_size, expected_line_height, description)
        (12.0, 13.0, "12px Liberation Serif"),
        (14.0, 16.0, "14px Liberation Serif"),  
        (16.0, 18.0, "16px Liberation Serif"),
        (18.0, 20.0, "18px Liberation Serif"),
        (20.0, 22.0, "20px Liberation Serif"),
        (24.0, 27.0, "24px Liberation Serif (Times New Roman)"),
    ];
    
    let mut passed = 0;
    let mut failed = 0;
    
    for (font_size, expected, desc) in test_cases {
        let mut style = ComputedStyle::default();
        style.font_size = font_size;
        style.font_family = Some("serif".to_string());
        style.line_height = None; // Use "normal"
        
        let metrics = measure_text("Test", &style);
        let actual = metrics.height;
        
        if actual == expected {
            println!("✅ PASS: {} → line-height: {} (expected: {})", desc, actual, expected);
            passed += 1;
        } else {
            println!("❌ FAIL: {} → line-height: {} (expected: {})", desc, actual, expected);
            failed += 1;
        }
    }
    
    println!("\n=== Results ===");
    println!("Passed: {}/{}", passed, passed + failed);
    println!("Failed: {}/{}", failed, passed + failed);
    
    if failed > 0 {
        println!("\n❌ Line-height fix FAILED verification");
        std::process::exit(1);
    } else {
        println!("\n✅ Line-height fix PASSED all tests!");
    }
}
