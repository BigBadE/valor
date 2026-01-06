# Fix get_single() -> single()
s/\.get_single()/\.single()/g
# Fix image.data to be Some(data)
s/image\.data = image_data;/image.data = Some(image_data);/g
# Fix Trigger -> On
s/Trigger<OnClick>/On<OnClick>/g
